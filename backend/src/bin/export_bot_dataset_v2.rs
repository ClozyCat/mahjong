use std::path::PathBuf;

use anyhow::{Context, Result};
use backend::bot_trainer::export::ExportOptions;

struct ExportArgs {
    input_path: PathBuf,
    output_dir: PathBuf,
    max_matches: Option<usize>,
    progress_every: Option<usize>,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let report = backend::bot_trainer::export::run_export_with_options(
        &args.input_path,
        &args.output_dir,
        ExportOptions {
            max_matches: args.max_matches,
            progress_every: args.progress_every,
        },
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    println!(
        "exported {} samples from {} matches to {}",
        report.sample_count,
        report.match_count,
        args.output_dir.display()
    );
    Ok(())
}

fn parse_args() -> Result<ExportArgs> {
    let mut input_path = PathBuf::from("backend/bot_trainer/dataset/data.txt");
    let mut output_dir = PathBuf::from("backend/bot_trainer/v2/out");
    let mut max_matches = None;
    let mut progress_every = Some(100_usize);

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => {
                input_path = PathBuf::from(args.next().context("--input requires a path")?)
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
        input_path,
        output_dir,
        max_matches,
        progress_every,
    })
}
