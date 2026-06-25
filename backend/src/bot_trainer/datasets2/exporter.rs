use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
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
    pub worker_count: usize,
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

    for result in process_files(
        collect_txt_files(input_dir, options.max_matches)?,
        options.worker_count,
    )? {
        if options
            .max_matches
            .is_some_and(|limit| report.match_count >= limit)
        {
            break;
        }
        write_processed_match(result, &mut writers, &mut report)?;
        maybe_report_progress(options.progress_every, report.match_count, &report, started);
    }

    writers.flush()?;
    write_json(output_dir.join("datasets2_export_report.json"), &report)?;
    Ok(report)
}

struct ProcessedDatasets2Match {
    source: String,
    match_counted: bool,
    split: Option<DatasetSplit>,
    samples: Result<Vec<super::super::replay::TrainingDecisionSampleV2>, String>,
}

fn process_files(
    paths: Vec<PathBuf>,
    worker_count: usize,
) -> Result<Vec<ProcessedDatasets2Match>, ExportError> {
    if worker_count <= 1 || paths.len() <= 1 {
        return paths.into_iter().map(process_one_file).collect();
    }

    let total = paths.len();
    let paths = Arc::new(paths);
    let workers = worker_count.min(total);
    let (sender, receiver) =
        mpsc::channel::<(usize, Result<ProcessedDatasets2Match, ExportError>)>();
    let mut handles = Vec::with_capacity(workers);
    for worker_index in 0..workers {
        let paths = Arc::clone(&paths);
        let sender = sender.clone();
        handles.push(thread::spawn(move || {
            let mut index = worker_index;
            while index < paths.len() {
                let processed = process_one_file(paths[index].clone());
                if sender.send((index, processed)).is_err() {
                    return;
                }
                index += workers;
            }
        }));
    }
    drop(sender);

    let mut ordered = std::iter::repeat_with(|| None)
        .take(total)
        .collect::<Vec<Option<Result<ProcessedDatasets2Match, ExportError>>>>();
    for (index, processed) in receiver {
        ordered[index] = Some(processed);
    }
    for handle in handles {
        if handle.join().is_err() {
            return Err(ExportError::Replay(
                "datasets2 export worker panicked".to_string(),
            ));
        }
    }

    ordered
        .into_iter()
        .map(|value| {
            value
                .ok_or_else(|| ExportError::Replay("missing datasets2 worker result".to_string()))?
        })
        .collect()
}

fn process_one_file(path: PathBuf) -> Result<ProcessedDatasets2Match, ExportError> {
    let source = path.display().to_string();
    let raw = fs::read_to_string(&path)?;
    let record = match parse_match_text(&raw, &source) {
        Ok(record) => record,
        Err(error) => {
            return Ok(ProcessedDatasets2Match {
                source,
                match_counted: false,
                split: None,
                samples: Err(error),
            });
        }
    };
    let split = split_for_match_id(&record.match_id);
    let samples = match replay_match_to_samples(&record) {
        Ok(samples) => Ok(samples),
        Err(error) => Err(error.to_string()),
    };
    Ok(ProcessedDatasets2Match {
        source,
        match_counted: true,
        split: Some(split),
        samples,
    })
}

fn write_processed_match(
    result: ProcessedDatasets2Match,
    writers: &mut ShardWriters,
    report: &mut Datasets2ExportReport,
) -> Result<(), ExportError> {
    if result.match_counted {
        report.match_count += 1;
    }
    let samples = match result.samples {
        Ok(samples) => samples,
        Err(error) => {
            if result.match_counted {
                report.replay_error_count += 1;
            } else {
                report.parse_error_count += 1;
            }
            report
                .skipped_files
                .push(format!("{}: {}", result.source, error));
            return Ok(());
        }
    };
    let split = result
        .split
        .ok_or_else(|| ExportError::Replay("missing split for datasets2 sample".to_string()))?;
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

fn collect_txt_files(
    input_dir: &Path,
    max_matches: Option<usize>,
) -> Result<Vec<PathBuf>, ExportError> {
    let mut pending = vec![input_dir.to_path_buf()];
    let mut paths = Vec::new();
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
            paths.push(entry_path);
            if max_matches.is_some_and(|limit| paths.len() >= limit) {
                return Ok(paths);
            }
        }
    }
    Ok(paths)
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
                worker_count: 0,
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

    #[test]
    fn parallel_datasets2_export_matches_single_worker_output() {
        let temp_root = std::env::temp_dir().join(format!(
            "mahjong-datasets2-workers-test-{}",
            std::process::id()
        ));
        let input_dir = temp_root.join("input").join("LIU");
        let single_dir = temp_root.join("single");
        let parallel_dir = temp_root.join("parallel");
        std::fs::create_dir_all(&input_dir).expect("create input");
        std::fs::write(input_dir.join("first.txt"), FIXTURE).expect("write first fixture");
        std::fs::write(
            input_dir.join("second.txt"),
            FIXTURE.replace("344397.xml", "344398.xml"),
        )
        .expect("write second fixture");

        run_export_directory(
            &temp_root.join("input"),
            &single_dir,
            ExportDirectoryOptions {
                max_matches: None,
                progress_every: None,
                worker_count: 0,
            },
        )
        .expect("single export");
        run_export_directory(
            &temp_root.join("input"),
            &parallel_dir,
            ExportDirectoryOptions {
                max_matches: None,
                progress_every: None,
                worker_count: 2,
            },
        )
        .expect("parallel export");

        for name in ["train.jsonl", "val.jsonl", "test.jsonl"] {
            assert_eq!(
                std::fs::read_to_string(single_dir.join(name)).expect("single split"),
                std::fs::read_to_string(parallel_dir.join(name)).expect("parallel split"),
            );
        }

        let _ = std::fs::remove_dir_all(temp_root);
    }
}
