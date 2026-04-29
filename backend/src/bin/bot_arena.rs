use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};

use anyhow::{Context, Result, anyhow, bail};
use backend::{
    bot::arena::{
        ArenaBotPolicyConfig, ArenaConfig, ArenaMatchAccumulator, ArenaMatchReport, arena_room,
        build_match_report,
    },
    core::engine::try_handle_player_action_in_room_state,
    rules::standard::{
        automation::next_bot_action_in_room_state_with_policy_resolver,
        flow::start_match_in_room_state,
    },
};

struct Args {
    config_path: PathBuf,
    output_path: PathBuf,
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let config: ArenaConfig = serde_json::from_str(
        &std::fs::read_to_string(&args.config_path)
            .with_context(|| format!("failed to read {}", args.config_path.display()))?,
    )?;
    let reports = run_arena(&config)?;
    let mut writer = BufWriter::new(File::create(&args.output_path)?);
    for report in reports {
        serde_json::to_writer(&mut writer, &report)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut config_path = None;
    let mut output_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config_path = args.next().map(PathBuf::from),
            "--output" => output_path = args.next().map(PathBuf::from),
            _ => bail!("unknown argument: {arg}"),
        }
    }
    Ok(Args {
        config_path: config_path.context("--config is required")?,
        output_path: output_path.context("--output is required")?,
    })
}

fn policy_for_seat(config: &ArenaConfig, seat_index: usize) -> ArenaBotPolicyConfig {
    config
        .policies
        .get(seat_index % config.policies.len())
        .cloned()
        .unwrap_or_else(ArenaBotPolicyConfig::heuristic)
}

fn run_arena(config: &ArenaConfig) -> Result<Vec<ArenaMatchReport>> {
    if config.policies.is_empty() {
        bail!("arena config requires at least one policy");
    }

    let mut reports = Vec::new();
    for match_index in 0..config.matches {
        let seed = config.seed.wrapping_add(match_index as u64);
        let mut room = arena_room(&format!("ARENA{match_index:04}"));
        start_match_in_room_state(&mut room, 0, seed).map_err(|reason| anyhow!(reason))?;
        let mut accumulator = ArenaMatchAccumulator::new(config);
        let mut action_count = 0_usize;

        while room.phase == "playing" && action_count < config.max_actions_per_match {
            let started = Instant::now();
            let action = next_bot_action_in_room_state_with_policy_resolver(&room, &|seat| {
                policy_for_seat(config, seat)
            })
            .map_err(|reason| anyhow!(reason))?;
            let Some(action) = action else {
                break;
            };
            let elapsed_ms = started.elapsed().as_millis();
            let output = try_handle_player_action_in_room_state(
                &mut room,
                action.seat_index,
                &action.action_type,
                &action.tile_ids,
            )
            .map_err(|reason| anyhow!(reason))?;
            match output {
                Some(Ok(_)) => {
                    accumulator.record_decision(
                        action.seat_index,
                        &action.action_type,
                        elapsed_ms,
                    );
                    action_count += 1;
                }
                Some(Err(reason)) => bail!("arena action was rejected: {reason}"),
                None => bail!(
                    "arena action was not handled: seat={} action={}",
                    action.seat_index,
                    action.action_type
                ),
            }
        }

        reports.push(build_match_report(
            match_index,
            seed,
            &room,
            accumulator,
            action_count,
            action_count < config.max_actions_per_match,
        ));
    }
    Ok(reports)
}
