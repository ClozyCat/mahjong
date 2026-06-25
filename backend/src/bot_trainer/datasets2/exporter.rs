use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::super::export::{
    DatasetSplit, ExportError, export_metadata, split_for_match_id, validate_sample,
};
use super::super::replay::replay_match_to_samples;
use super::parser::parse_match_text;

#[derive(Debug, Clone, Copy, Default)]
pub struct ExportDirectoryOptions {
    pub max_matches: Option<usize>,
    pub progress_every: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Datasets2ExportReport {
    pub match_count: usize,
    pub sample_count: usize,
    pub parse_error_count: usize,
    pub replay_error_count: usize,
    pub runtime_illegal_label_count: usize,
    pub skipped_files: Vec<String>,
}

pub fn run_export_directory(
    input_dir: &Path,
    output_dir: &Path,
    options: ExportDirectoryOptions,
) -> Result<Datasets2ExportReport, ExportError> {
    let started = Instant::now();
    fs::create_dir_all(output_dir)?;
    write_json(output_dir.join("metadata.json"), &export_metadata())?;
    let mut writers = ShardWriters::new(output_dir)?;
    let mut report = Datasets2ExportReport::default();

    walk_txt_files(input_dir, |path| {
        if options
            .max_matches
            .is_some_and(|limit| report.match_count >= limit)
        {
            return Ok(WalkControl::Stop);
        }
        export_one_match(path, &mut writers, &mut report)?;
        maybe_report_progress(options.progress_every, report.match_count, &report, started);
        Ok(WalkControl::Continue)
    })?;

    writers.flush()?;
    write_json(output_dir.join("datasets2_export_report.json"), &report)?;
    Ok(report)
}

fn export_one_match(
    path: &Path,
    writers: &mut ShardWriters,
    report: &mut Datasets2ExportReport,
) -> Result<(), ExportError> {
    let source = path.display().to_string();
    let raw = fs::read_to_string(path)?;
    let record = match parse_match_text(&raw, &source) {
        Ok(record) => record,
        Err(error) => {
            report.parse_error_count += 1;
            report.skipped_files.push(format!("{source}: {error}"));
            return Ok(());
        }
    };
    report.match_count += 1;
    let split = split_for_match_id(&record.match_id);
    let samples = match replay_match_to_samples(&record) {
        Ok(samples) => samples,
        Err(error) => {
            report.replay_error_count += 1;
            report.skipped_files.push(format!("{source}: {error}"));
            return Ok(());
        }
    };
    for sample in samples {
        if validate_sample(&sample).is_err() {
            report.runtime_illegal_label_count += 1;
            continue;
        }
        writers.write(split, &sample)?;
        report.sample_count += 1;
    }
    Ok(())
}

enum WalkControl {
    Continue,
    Stop,
}

fn walk_txt_files(
    input_dir: &Path,
    mut visit: impl FnMut(&Path) -> Result<WalkControl, ExportError>,
) -> Result<(), ExportError> {
    let mut pending = vec![input_dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries.into_iter().rev() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                pending.push(entry_path);
                continue;
            }
            if !entry_path.extension().is_some_and(|ext| ext == "txt") {
                continue;
            }
            if matches!(visit(&entry_path)?, WalkControl::Stop) {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn maybe_report_progress(
    progress_every: Option<usize>,
    processed: usize,
    report: &Datasets2ExportReport,
    started: Instant,
) {
    let Some(progress_every) = progress_every.filter(|value| *value > 0) else {
        return;
    };
    if processed == 0 || processed % progress_every != 0 {
        return;
    }
    eprintln!(
        "datasets2 progress: matches={} samples={} parse_errors={} replay_errors={} elapsed_s={:.1}",
        processed,
        report.sample_count,
        report.parse_error_count,
        report.replay_error_count,
        started.elapsed().as_secs_f64(),
    );
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<(), ExportError> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

struct ShardWriters {
    train: BufWriter<File>,
    val: BufWriter<File>,
    test: BufWriter<File>,
}

impl ShardWriters {
    fn new(output_dir: &Path) -> Result<Self, ExportError> {
        Ok(Self {
            train: BufWriter::new(File::create(output_dir.join("train.jsonl"))?),
            val: BufWriter::new(File::create(output_dir.join("val.jsonl"))?),
            test: BufWriter::new(File::create(output_dir.join("test.jsonl"))?),
        })
    }

    fn write(&mut self, split: DatasetSplit, sample: &impl Serialize) -> Result<(), ExportError> {
        let writer = match split {
            DatasetSplit::Train => &mut self.train,
            DatasetSplit::Val => &mut self.val,
            DatasetSplit::Test => &mut self.test,
        };
        serde_json::to_writer(&mut *writer, sample)?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ExportError> {
        self.train.flush()?;
        self.val.flush()?;
        self.test.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_trainer::datasets2::test_support::FIXTURE;

    #[test]
    fn exports_datasets2_directory_to_sft_jsonl() {
        let temp_root =
            std::env::temp_dir().join(format!("mahjong-datasets2-test-{}", std::process::id()));
        let input_dir = temp_root.join("input").join("LIU");
        let output_dir = temp_root.join("out");
        std::fs::create_dir_all(&input_dir).expect("create input");
        std::fs::write(input_dir.join("fixture.txt"), FIXTURE).expect("write fixture");

        let report = run_export_directory(
            &temp_root.join("input"),
            &output_dir,
            ExportDirectoryOptions {
                max_matches: None,
                progress_every: None,
            },
        )
        .expect("export succeeds");

        assert_eq!(report.match_count, 1);
        assert!(report.sample_count > 0);
        assert!(output_dir.join("metadata.json").is_file());
        assert!(output_dir.join("train.jsonl").is_file());
        assert!(output_dir.join("val.jsonl").is_file());
        assert!(output_dir.join("test.jsonl").is_file());

        let total_rows = ["train.jsonl", "val.jsonl", "test.jsonl"]
            .into_iter()
            .map(|name| {
                std::fs::read_to_string(output_dir.join(name))
                    .expect("read split")
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .count()
            })
            .sum::<usize>();
        assert_eq!(total_rows, report.sample_count);

        let _ = std::fs::remove_dir_all(temp_root);
    }
}
