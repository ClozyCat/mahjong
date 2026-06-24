use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use backend::{bot::arena::run_evaluation_arena_with_options, evaluation::EvaluationArenaConfig};

struct Args {
    config_path: PathBuf,
    output_path: PathBuf,
    trajectories_path: Option<PathBuf>,
    counterfactual_discards_path: Option<PathBuf>,
    jobs: usize,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let config: EvaluationArenaConfig = serde_json::from_str(
        &std::fs::read_to_string(&args.config_path)
            .with_context(|| format!("failed to read {}", args.config_path.display()))?,
    )?;
    let include_trajectories = args.trajectories_path.is_some();
    let include_counterfactual_discards = args.counterfactual_discards_path.is_some();
    let arena_output = run_evaluation_arena_with_options(
        &config,
        include_trajectories,
        include_counterfactual_discards,
        args.jobs,
    )
    .map_err(|reason| anyhow::anyhow!(reason))?;

    let mut report_writer = BufWriter::new(File::create(&args.output_path)?);
    for report in arena_output.reports {
        serde_json::to_writer(&mut report_writer, &report)?;
        report_writer.write_all(b"\n")?;
    }

    if let Some(trajectories_path) = args.trajectories_path {
        let mut trajectory_writer = BufWriter::new(File::create(trajectories_path)?);
        for row in arena_output.trajectories {
            serde_json::to_writer(&mut trajectory_writer, &row)?;
            trajectory_writer.write_all(b"\n")?;
        }
    }

    if let Some(counterfactual_discards_path) = args.counterfactual_discards_path {
        let mut writer = BufWriter::new(File::create(counterfactual_discards_path)?);
        for row in arena_output.counterfactual_discards {
            serde_json::to_writer(&mut writer, &row)?;
            writer.write_all(b"\n")?;
        }
    }

    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut config_path = None;
    let mut output_path = None;
    let mut trajectories_path = None;
    let mut counterfactual_discards_path = None;
    let mut jobs = 1_usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config_path = args.next().map(PathBuf::from),
            "--output" => output_path = args.next().map(PathBuf::from),
            "--trajectories" => trajectories_path = args.next().map(PathBuf::from),
            "--counterfactual-discards" => {
                counterfactual_discards_path = args.next().map(PathBuf::from)
            }
            "--jobs" => {
                let raw = args.next().context("--jobs requires a value")?;
                jobs = raw
                    .parse::<usize>()
                    .with_context(|| format!("invalid --jobs value: {raw}"))?;
                if jobs == 0 {
                    jobs = std::thread::available_parallelism()
                        .map(usize::from)
                        .unwrap_or(1);
                }
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    Ok(Args {
        config_path: config_path.context("--config is required")?,
        output_path: output_path.context("--output is required")?,
        trajectories_path,
        counterfactual_discards_path,
        jobs,
    })
}
