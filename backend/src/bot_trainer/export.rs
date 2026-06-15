use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::botzone::parse_matches;
use super::replay::{
    DecisionKind, TrainingDecisionSampleV2, TrainingLabel, replay_match_to_samples,
};
use crate::bot::action_space::{CLAIM_ACTIONS, SELF_KONG_ACTIONS, TILE_KEYS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DatasetSplit {
    Train,
    Val,
    Test,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotDatasetMetadata {
    pub schema_version: u32,
    pub tile_keys: Vec<String>,
    pub decision_kinds: Vec<String>,
    pub claim_actions: Vec<String>,
    pub self_kong_actions: Vec<String>,
    pub model_outputs: Vec<String>,
    pub split_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportReport {
    pub match_count: usize,
    pub sample_count: usize,
    pub illegal_label_count: usize,
    pub runtime_illegal_label_count: usize,
    pub parse_error_count: usize,
    pub skipped_match_ids: Vec<String>,
    pub samples_by_split: BTreeMap<DatasetSplit, usize>,
    pub samples_by_decision_kind: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExportOptions {
    pub max_matches: Option<usize>,
    pub progress_every: Option<usize>,
}

#[derive(Debug)]
pub enum ExportError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Parse(String),
    Replay(String),
    IllegalLabel {
        match_id: String,
        decision_index: u64,
        label: String,
        legal_actions: Vec<String>,
    },
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "io error: {error}"),
            Self::Json(error) => write!(formatter, "json error: {error}"),
            Self::Parse(error) => write!(formatter, "parse error: {error}"),
            Self::Replay(error) => write!(formatter, "replay error: {error}"),
            Self::IllegalLabel {
                match_id,
                decision_index,
                label,
                legal_actions,
            } => write!(
                formatter,
                "illegal label {label} at {match_id}#{decision_index}; legal={legal_actions:?}"
            ),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<std::io::Error> for ExportError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ExportError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub fn run_export(
    input_path: &Path,
    output_dir: &Path,
    max_matches: Option<usize>,
) -> Result<ExportReport, ExportError> {
    run_export_with_options(
        input_path,
        output_dir,
        ExportOptions {
            max_matches,
            progress_every: None,
        },
    )
}

pub fn run_export_with_options(
    input_path: &Path,
    output_dir: &Path,
    options: ExportOptions,
) -> Result<ExportReport, ExportError> {
    let started = Instant::now();
    eprintln!("reading dataset: {}", input_path.display());
    let raw = fs::read_to_string(input_path)?;
    eprintln!(
        "parsing matches: {:.1} MB loaded",
        raw.len() as f64 / 1024.0 / 1024.0
    );
    let mut matches = parse_matches(&raw).map_err(|error| ExportError::Parse(error.to_string()))?;
    if let Some(limit) = options.max_matches {
        matches.truncate(limit);
    }
    eprintln!("parsed {} matches", matches.len());

    fs::create_dir_all(output_dir)?;
    write_json(output_dir.join("metadata.json"), &export_metadata())?;

    let mut writers = ShardWriters::new(output_dir)?;
    let mut report = ExportReport {
        match_count: matches.len(),
        ..ExportReport::default()
    };
    let total_matches = matches.len();

    for (match_offset, record) in matches.into_iter().enumerate() {
        let split = split_for_match_id(&record.match_id);
        let samples = match replay_match_to_samples(&record) {
            Ok(samples) => samples,
            Err(error) => {
                report.parse_error_count += 1;
                report.skipped_match_ids.push(record.match_id);
                eprintln!("skip match after replay error: {error}");
                continue;
            }
        };
        for sample in samples {
            if validate_sample(&sample).is_err() {
                report.runtime_illegal_label_count += 1;
                continue;
            }
            *report.samples_by_split.entry(split).or_default() += 1;
            *report
                .samples_by_decision_kind
                .entry(decision_kind_name(&sample.decision_kind).to_string())
                .or_default() += 1;
            writers.write(split, &sample)?;
            report.sample_count += 1;
        }
        maybe_report_progress(
            options.progress_every,
            match_offset + 1,
            total_matches,
            &report,
            started,
        );
    }

    writers.flush()?;
    write_json(output_dir.join("export_report.json"), &report)?;
    eprintln!(
        "finished export: matches={} samples={} skipped={} illegal_labels={} runtime_illegal_labels={} elapsed_s={:.1}",
        report.match_count,
        report.sample_count,
        report.skipped_match_ids.len(),
        report.illegal_label_count,
        report.runtime_illegal_label_count,
        started.elapsed().as_secs_f64()
    );
    Ok(report)
}

pub fn split_for_match_id(match_id: &str) -> DatasetSplit {
    let bucket = stable_hash(match_id) % 100;
    match bucket {
        0..=79 => DatasetSplit::Train,
        80..=89 => DatasetSplit::Val,
        _ => DatasetSplit::Test,
    }
}

pub fn export_metadata() -> BotDatasetMetadata {
    BotDatasetMetadata {
        schema_version: 6,
        tile_keys: TILE_KEYS.iter().map(|value| (*value).to_string()).collect(),
        decision_kinds: ["active_turn", "claim_window", "rob_kong"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        claim_actions: CLAIM_ACTIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        self_kong_actions: SELF_KONG_ACTIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        model_outputs: [
            "discard_logits",
            "claim_logits",
            "self_kong_logits",
            "hu_logits",
            "value",
            "value_for_risk",
            "fan_value",
            "qualifying_fan_value",
            "opponent_tenpai_logits",
            "opponent_risk_logits",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        split_strategy: "match_id_hash".to_string(),
    }
}

pub(crate) fn validate_sample(sample: &TrainingDecisionSampleV2) -> Result<(), ExportError> {
    let label_action = label_to_action_id(&sample.label);
    if !sample
        .legal_actions
        .iter()
        .any(|action| action == &label_action)
    {
        return Err(ExportError::IllegalLabel {
            match_id: sample.match_id.clone(),
            decision_index: sample.decision_index,
            label: label_action,
            legal_actions: sample.legal_actions.clone(),
        });
    }
    Ok(())
}

pub(crate) fn label_to_action_id(label: &TrainingLabel) -> String {
    match label {
        TrainingLabel::Discard { tile_key } => format!("discard:{tile_key}"),
        TrainingLabel::ClaimChow { middle_tile_key } => format!("claim:chow:{middle_tile_key}"),
        TrainingLabel::ClaimPung { tile_key } => format!("claim:pung:{tile_key}"),
        TrainingLabel::ClaimKong { tile_key } => format!("claim:kong:{tile_key}"),
        TrainingLabel::SelfKong { kind, tile_key } => format!("self_kong:{kind}:{tile_key}"),
        TrainingLabel::Hu => "claim:hu".to_string(),
        TrainingLabel::Pass => "pass".to_string(),
    }
}

fn maybe_report_progress(
    progress_every: Option<usize>,
    processed_matches: usize,
    total_matches: usize,
    report: &ExportReport,
    started: Instant,
) {
    let Some(progress_every) = progress_every.filter(|value| *value > 0) else {
        return;
    };
    if processed_matches != total_matches && processed_matches % progress_every != 0 {
        return;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let matches_per_second = processed_matches as f64 / elapsed.max(0.001);
    let remaining = total_matches.saturating_sub(processed_matches);
    let eta_seconds = remaining as f64 / matches_per_second.max(0.001);
    eprintln!(
        "progress: {processed_matches}/{total_matches} matches ({:.1}%) samples={} skipped={} illegal_labels={} elapsed_s={elapsed:.1} eta_s={eta_seconds:.1}",
        processed_matches as f64 * 100.0 / total_matches.max(1) as f64,
        report.sample_count,
        report.skipped_match_ids.len(),
        report.illegal_label_count,
    );
}

fn stable_hash(value: &str) -> u64 {
    value
        .bytes()
        .fold(14_695_981_039_346_656_037_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211)
        })
}

fn decision_kind_name(kind: &DecisionKind) -> &'static str {
    match kind {
        DecisionKind::ActiveTurn => "active_turn",
        DecisionKind::ClaimWindow => "claim_window",
        DecisionKind::RobKong => "rob_kong",
    }
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

    fn write(
        &mut self,
        split: DatasetSplit,
        sample: &TrainingDecisionSampleV2,
    ) -> Result<(), ExportError> {
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

    #[test]
    fn split_assignment_is_stable_by_match_id() {
        let first = split_for_match_id("61602cb45ddc087351c04358");
        let second = split_for_match_id("61602cb45ddc087351c04358");
        assert_eq!(first, second);
    }

    #[test]
    fn metadata_contains_v6_model_outputs() {
        let metadata = export_metadata();
        assert_eq!(metadata.schema_version, 6);
        assert!(
            metadata
                .model_outputs
                .contains(&"discard_logits".to_string())
        );
        assert!(metadata.model_outputs.contains(&"claim_logits".to_string()));
        assert!(
            metadata
                .model_outputs
                .contains(&"opponent_tenpai_logits".to_string())
        );
        assert!(
            metadata
                .model_outputs
                .contains(&"opponent_risk_logits".to_string())
        );
        assert!(metadata.model_outputs.contains(&"fan_value".to_string()));
        assert!(
            metadata
                .model_outputs
                .contains(&"value_for_risk".to_string())
        );
        assert!(
            metadata
                .model_outputs
                .contains(&"qualifying_fan_value".to_string())
        );
    }

    #[test]
    fn label_ids_match_replay_legal_action_format() {
        assert_eq!(
            label_to_action_id(&TrainingLabel::Discard {
                tile_key: "w1".to_string()
            }),
            "discard:w1"
        );
        assert_eq!(
            label_to_action_id(&TrainingLabel::ClaimPung {
                tile_key: "red".to_string()
            }),
            "claim:pung:red"
        );
        assert_eq!(label_to_action_id(&TrainingLabel::Hu), "claim:hu");
        assert_eq!(label_to_action_id(&TrainingLabel::Pass), "pass");
    }

    #[test]
    fn validate_sample_rejects_restricted_discard_label() {
        let mut sample = TrainingDecisionSampleV2 {
            schema_version: 2,
            match_id: "fixture".to_string(),
            decision_index: 0,
            seat_index: 0,
            decision_kind: DecisionKind::ActiveTurn,
            context: crate::bot_trainer::replay::SerializableBotContext {
                seat_index: 0,
                seat_count: 4,
                dealer_seat: 0,
                seat_wind: Some("east".to_string()),
                round_wind: "east".to_string(),
                cumulative_scores: vec![0, 0, 0, 0],
                wall_tiles_remaining: 70,
                visible_tile_keys: vec![],
                opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
                opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
                discard_history: vec![],
                player: crate::bot_trainer::replay::SerializableBotPlayer {
                    concealed_tiles: vec![],
                    concealed_tile_counts: vec![0; 34],
                    meld_tile_key_groups: vec![],
                    flower_count: 0,
                },
                restricted_discard_tile_key: Some("w1".to_string()),
                drawn_tile_id: None,
                self_kong_candidates: vec![],
                claim_options: vec![],
                last_discard_tile_key: None,
                add_kong_risk_tiles: Default::default(),
            },
            legal_actions: vec!["discard:w2".to_string()],
            label: TrainingLabel::Discard {
                tile_key: "w1".to_string(),
            },
            outcome: crate::bot_trainer::replay::SampleOutcome {
                score_delta: 0,
                fan_count: 0,
                won: false,
                dealt_in: false,
                round_drawn: false,
            },
            opponent_tenpai_target: vec![0.0; 3],
            opponent_risk_target: vec![vec![0.0; TILE_KEYS.len()]; 3],
            opponent_risk_mask: vec![vec![0.0; TILE_KEYS.len()]; 3],
        };

        assert!(validate_sample(&sample).is_err());
        sample.label = TrainingLabel::Discard {
            tile_key: "w2".to_string(),
        };
        assert!(validate_sample(&sample).is_ok());
    }
}
