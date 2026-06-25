use std::path::PathBuf;

use anyhow::{Context, Result};
use backend::bot_trainer::datasets2::ExportDirectoryOptions;

struct ExportArgs {
    input_dir: PathBuf,
    output_dir: PathBuf,
    max_matches: Option<usize>,
    progress_every: Option<usize>,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let report = backend::bot_trainer::datasets2::run_export_directory(
        &args.input_dir,
        &args.output_dir,
        ExportDirectoryOptions {
            max_matches: args.max_matches,
            progress_every: args.progress_every,
        },
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    println!(
        "exported {} samples from {} datasets2 matches to {}",
        report.sample_count,
        report.match_count,
        args.output_dir.display()
    );
    if report.parse_error_count > 0 || report.replay_error_count > 0 {
        println!(
            "skipped parse_errors={} replay_errors={} runtime_illegal_labels={}",
            report.parse_error_count, report.replay_error_count, report.runtime_illegal_label_count,
        );
    }
    Ok(())
}

fn parse_args() -> Result<ExportArgs> {
    let mut input_dir = PathBuf::from("backend/bot_trainer/datasets2");
    let mut output_dir = PathBuf::from("backend/bot_trainer/v2/sft/out_datasets2");
    let mut max_matches = None;
    let mut progress_every = Some(10_000_usize);

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => {
                input_dir = PathBuf::from(args.next().context("--input requires a directory")?)
            }
            "--output" => {
                output_dir = PathBuf::from(args.next().context("--output requires a path")?)
            }
            "--max-matches" => {
                let value = args.next().context("--max-matches requires a number")?;
                max_matches = Some(value.parse::<usize>()?);
            }
            "--progress-every" => {
                let value = args.next().context("--progress-every requires a number")?;
                progress_every = Some(value.parse::<usize>()?);
            }
            "--no-progress" => {
                progress_every = None;
            }
            _ => anyhow::bail!("unknown argument: {arg}"),
        }
    }

    Ok(ExportArgs {
        input_dir,
        output_dir,
        max_matches,
        progress_every,
    })
}
