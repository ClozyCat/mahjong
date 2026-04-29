use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use backend::bot::arena::{
    ArenaConfig, run_arena, run_arena_parallel_with_progress, run_arena_with_progress,
};

struct Args {
    config_path: PathBuf,
    output_path: PathBuf,
    trajectories_path: Option<PathBuf>,
    progress_every: Option<usize>,
    jobs: Option<usize>,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let config: ArenaConfig = serde_json::from_str(
        &std::fs::read_to_string(&args.config_path)
            .with_context(|| format!("failed to read {}", args.config_path.display()))?,
    )?;
    let include_trajectories = args.trajectories_path.is_some();
    let arena_output = if args.jobs.is_some() || args.progress_every.is_some() {
        let worker_count = resolve_worker_count(args.jobs);
        let total_matches = config.matches;
        let mut completed_count = 0_usize;
        let progress_every = args.progress_every;
        let mut on_match_complete = |report: &backend::bot::arena::ArenaMatchReport| {
            completed_count += 1;
            if let Some(progress_every) = progress_every {
                if completed_count % progress_every == 0 || completed_count == total_matches {
                    eprintln!(
                        "Arena progress: completed {}/{} match={} seed={} actions={} completed={} jobs={}",
                        completed_count,
                        total_matches,
                        report.match_index + 1,
                        report.seed,
                        report.action_count,
                        report.completed,
                        worker_count
                    );
                }
            }
        };
        if worker_count > 1 {
            run_arena_parallel_with_progress(
                &config,
                include_trajectories,
                worker_count,
                &mut on_match_complete,
            )
            .map_err(|reason| anyhow::anyhow!(reason))?
        } else {
            run_arena_with_progress(&config, include_trajectories, &mut on_match_complete)
                .map_err(|reason| anyhow::anyhow!(reason))?
        }
    } else {
        run_arena(&config, include_trajectories).map_err(|reason| anyhow::anyhow!(reason))?
    };

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

    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut config_path = None;
    let mut output_path = None;
    let mut trajectories_path = None;
    let mut progress_every = None;
    let mut jobs = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config_path = args.next().map(PathBuf::from),
            "--output" => output_path = args.next().map(PathBuf::from),
            "--trajectories" => trajectories_path = args.next().map(PathBuf::from),
            "--progress-every" => {
                let raw = args.next().context("--progress-every requires a value")?;
                let parsed = raw
                    .parse::<usize>()
                    .with_context(|| format!("invalid --progress-every value: {raw}"))?;
                if parsed == 0 {
                    bail!("--progress-every must be greater than 0");
                }
                progress_every = Some(parsed);
            }
            "--jobs" => {
                let raw = args.next().context("--jobs requires a value")?;
                jobs = Some(
                    raw.parse::<usize>()
                        .with_context(|| format!("invalid --jobs value: {raw}"))?,
                );
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    Ok(Args {
        config_path: config_path.context("--config is required")?,
        output_path: output_path.context("--output is required")?,
        trajectories_path,
        progress_every,
        jobs,
    })
}

fn resolve_worker_count(jobs: Option<usize>) -> usize {
    match jobs {
        Some(0) => std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1),
        Some(jobs) => jobs,
        None => 1,
    }
}
