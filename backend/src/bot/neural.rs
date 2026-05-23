use super::action_space::{CLAIM_ACTION_COUNT, SELF_KONG_ACTION_COUNT, TILE_KIND_COUNT};
#[cfg(test)]
use super::action_space::{CLAIM_ACTIONS, tile_index};
use super::context::BotContext;
#[cfg(test)]
use super::context::BotTileView;
use super::features::{
    BotFeaturesV2, discard_event_feature_count_v2, discard_sequence_length_v2,
    encode_bot_context_v2, scalar_feature_count_v2, tile_plane_count_v2,
};
use ort::{session::Session, value::Tensor};
use std::{
    cell::RefCell,
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const DEFAULT_MODEL_PATHS: [&str; 4] = [
    "/app/assets/sft/sft.onnx",
    "assets/sft/sft.onnx",
    "backend/assets/sft/sft.onnx",
    "sft.onnx",
];
const MODEL_PATH_ENV: &str = "MAHJONG_BOT_MODEL_PATH";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NeuralDecisionScores {
    pub(crate) discard_logits: [f32; TILE_KIND_COUNT],
    pub(crate) claim_logits: [f32; CLAIM_ACTION_COUNT],
    pub(crate) self_kong_logits: [f32; SELF_KONG_ACTION_COUNT],
    pub(crate) hu_logits: [f32; 2],
    pub(crate) value: f32,
    pub(crate) risk_logits: [f32; TILE_KIND_COUNT],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RankedTileScore {
    pub(crate) tile_id: String,
    pub(crate) tile_key: String,
    pub(crate) logit: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RankedClaimScore {
    pub(crate) action_name: &'static str,
    pub(crate) logit: f32,
}

pub(crate) fn neural_decision_scores_for_model_path(
    context: &BotContext,
    model_path: Option<&Path>,
) -> Option<NeuralDecisionScores> {
    let features = encode_bot_context_v2(context);
    neural_decision_scores_for_features(model_path, features)
}

pub(crate) fn neural_decision_scores_for_features(
    model_path: Option<&Path>,
    features: BotFeaturesV2,
) -> Option<NeuralDecisionScores> {
    if let Some(path) = model_path {
        let path = resolve_model_path(path);
        return CACHED_SESSIONS
            .with(|sessions| {
                let session = *sessions
                    .borrow_mut()
                    .entry(path.clone())
                    .or_insert_with(|| {
                        // ORT Session destruction can block during arena worker teardown on
                        // Windows. Arena processes are short-lived, so keep per-thread sessions
                        // alive until process exit and let the OS reclaim them.
                        Box::leak(Box::new(RefCell::new(OrtNeuralSession::new(path.clone()))))
                    });
                session.borrow_mut().run(features)
            })
            .ok();
    }
    shared_session().lock().ok()?.run(features).ok()
}

thread_local! {
    static CACHED_SESSIONS: RefCell<HashMap<PathBuf, &'static RefCell<OrtNeuralSession>>> =
        RefCell::new(HashMap::new());
}

#[cfg(test)]
fn rank_masked_discards(
    context: &BotContext,
    logits: &[f32; TILE_KIND_COUNT],
) -> Vec<RankedTileScore> {
    let features = encode_bot_context_v2(context);
    let mut scores = Vec::new();
    let mut visited_tile_keys = std::collections::HashSet::new();
    for tile in &context.player.concealed_tiles {
        if tile.is_flower || !visited_tile_keys.insert(tile.tile_key.clone()) {
            continue;
        }
        let Some(index) = tile_index(&tile.tile_key) else {
            continue;
        };
        if !features.discard_mask[index] {
            continue;
        }
        let Some(tile_id) =
            preferred_discard_tile_id_for_key(&context.player.concealed_tiles, &tile.tile_key)
        else {
            continue;
        };
        scores.push(RankedTileScore {
            tile_id,
            tile_key: tile.tile_key.clone(),
            logit: logits[index],
        });
    }
    scores.sort_by(|left, right| {
        right
            .logit
            .total_cmp(&left.logit)
            .then_with(|| right.tile_key.cmp(&left.tile_key))
    });
    scores
}

#[cfg(test)]
fn rank_masked_claims(
    context: &BotContext,
    logits: &[f32; CLAIM_ACTION_COUNT],
) -> Vec<RankedClaimScore> {
    let features = encode_bot_context_v2(context);
    let mut scores = CLAIM_ACTIONS
        .iter()
        .enumerate()
        .filter_map(|(index, action_name)| {
            features.claim_mask[index].then_some(RankedClaimScore {
                action_name,
                logit: logits[index],
            })
        })
        .collect::<Vec<_>>();
    scores.sort_by(|left, right| {
        right
            .logit
            .total_cmp(&left.logit)
            .then_with(|| right.action_name.cmp(left.action_name))
    });
    scores
}

fn shared_session() -> &'static Mutex<OrtNeuralSession> {
    static SESSION: OnceLock<Mutex<OrtNeuralSession>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(OrtNeuralSession::new(model_path_from_env())))
}

fn model_path_from_env() -> PathBuf {
    if let Some(path) = env::var_os(MODEL_PATH_ENV) {
        return resolve_model_path(Path::new(&path));
    }
    DEFAULT_MODEL_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_PATHS[0]))
}

fn resolve_model_path(path: &Path) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }

    if let Ok(relative) = path.strip_prefix("backend") {
        let asset_path = PathBuf::from(relative);
        if asset_path.exists() {
            return asset_path;
        }
        let app_asset_path = PathBuf::from("/app").join(relative);
        if app_asset_path.exists() {
            return app_asset_path;
        }
    }

    path.to_path_buf()
}

struct OrtNeuralSession {
    model_path: PathBuf,
    session: Option<Session>,
    disabled: bool,
    #[cfg(test)]
    load_attempts: usize,
}

impl OrtNeuralSession {
    fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            session: None,
            disabled: false,
            #[cfg(test)]
            load_attempts: 0,
        }
    }

    fn run(&mut self, features: BotFeaturesV2) -> Result<NeuralDecisionScores, ()> {
        if self.disabled {
            return Err(());
        }
        if self.session.is_none() {
            #[cfg(test)]
            {
                self.load_attempts += 1;
            }
            self.session = match load_session(&self.model_path) {
                Ok(session) => Some(session),
                Err(_) => {
                    self.disabled = true;
                    return Err(());
                }
            };
        }
        run_session(
            self.session.as_mut().expect("session initialized"),
            features,
        )
    }
}

fn build_session_builder() -> Result<ort::session::builder::SessionBuilder, ()> {
    Session::builder()
        .map_err(|_| ())?
        .with_intra_threads(1)
        .map_err(|_| ())?
        .with_inter_threads(1)
        .map_err(|_| ())?
        .with_intra_op_spinning(false)
        .map_err(|_| ())?
        .with_inter_op_spinning(false)
        .map_err(|_| ())?
        .with_flush_to_zero()
        .map_err(|_| ())
}

fn load_session(model_path: &Path) -> Result<Session, ()> {
    #[cfg(feature = "cuda")]
    {
        let r = build_session_builder()
            .and_then(|b| {
                b.with_execution_providers(
                    [ort::execution_providers::CUDAExecutionProvider::default().into()],
                )
                .map_err(|_| ())
            })
            .and_then(|mut b| b.commit_from_file(model_path).map_err(|_| ()));
        if r.is_ok() {
            eprintln!("[neural] CUDA EP loaded for: {}", model_path.display());
            return r;
        }
        eprintln!("[neural] CUDA EP failed, falling back to CPU: {}", model_path.display());
    }
    build_session_builder()?
        .commit_from_file(model_path)
        .map_err(|_| ())
}

fn run_session(session: &mut Session, features: BotFeaturesV2) -> Result<NeuralDecisionScores, ()> {
    let BotFeaturesV2 {
        tile_planes,
        scalar_features,
        discard_sequence,
        self_kong_mask: _self_kong_mask,
        hu_mask: _hu_mask,
        ..
    } = features;
    let tile_planes = Tensor::from_array((
        [1_usize, tile_plane_count_v2(), TILE_KIND_COUNT],
        tile_planes,
    ))
    .map_err(|_| ())?;
    let scalar_features =
        Tensor::from_array(([1_usize, scalar_feature_count_v2()], scalar_features))
            .map_err(|_| ())?;
    let discard_sequence = Tensor::from_array((
        [
            1_usize,
            discard_sequence_length_v2(),
            discard_event_feature_count_v2(),
        ],
        discard_sequence,
    ))
    .map_err(|_| ())?;
    let outputs = session
        .run(ort::inputs![
            "tile_planes" => tile_planes,
            "scalar_features" => scalar_features,
            "discard_sequence" => discard_sequence
        ])
        .map_err(|_| ())?;

    Ok(NeuralDecisionScores {
        discard_logits: extract_array::<TILE_KIND_COUNT>(&outputs, "discard_logits")?,
        claim_logits: extract_array::<CLAIM_ACTION_COUNT>(&outputs, "claim_logits")?,
        self_kong_logits: extract_array::<SELF_KONG_ACTION_COUNT>(&outputs, "self_kong_logits")?,
        hu_logits: extract_array::<2>(&outputs, "hu_logits")?,
        value: extract_array::<1>(&outputs, "value")?[0],
        risk_logits: extract_array::<TILE_KIND_COUNT>(&outputs, "risk_logits")?,
    })
}

fn extract_array<const N: usize>(
    outputs: &ort::session::SessionOutputs,
    output_name: &str,
) -> Result<[f32; N], ()> {
    let output = if outputs.contains_key(output_name) {
        &outputs[output_name]
    } else {
        return Err(());
    };
    let (_, values) = output.try_extract_tensor::<f32>().map_err(|_| ())?;
    values.get(0..N).ok_or(())?.try_into().map_err(|_| ())
}

#[cfg(test)]
fn preferred_discard_tile_id_for_key(
    concealed_tiles: &[BotTileView],
    tile_key: &str,
) -> Option<String> {
    concealed_tiles
        .iter()
        .find(|tile| !tile.is_flower && tile.tile_key == tile_key)
        .map(|tile| tile.tile_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::action_space::claim_action_index;
    use crate::projection::bot_view::{BotContextView, BotPlayerView, BotTileView};
    use std::{collections::HashSet, fs};

    fn tile(tile_id: &str, tile_key: &str) -> BotTileView {
        BotTileView {
            tile_id: tile_id.to_string(),
            tile_key: tile_key.to_string(),
            is_flower: false,
        }
    }

    fn base_context() -> BotContextView {
        BotContextView {
            seat_index: 0,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: Some("east".to_string()),
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: 42,
            visible_tile_keys: Vec::new(),
            opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
            opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
            discard_history: Vec::new(),
            kong_entries: Vec::new(),
            player: BotPlayerView {
                concealed_tiles: Vec::new(),
                concealed_tile_counts: [0; 34],
                meld_tile_key_groups: Vec::new(),
                flower_count: 0,
            },
            restricted_discard_tile_key: None,
            drawn_tile_id: None,
            self_kong_candidates: Vec::new(),
            claim_options: Vec::new(),
            last_discard_tile_key: None,
            add_kong_risk_tiles: HashSet::new(),
        }
    }

    #[test]
    fn v2_session_outputs_named_multi_head_scores() {
        let context = base_context();
        let scores = neural_decision_scores_for_model_path(&context, None);

        if let Some(scores) = scores {
            assert_eq!(scores.discard_logits.len(), 34);
            assert_eq!(scores.claim_logits.len(), 7);
            assert_eq!(scores.self_kong_logits.len(), 3);
            assert_eq!(scores.hu_logits.len(), 2);
            assert_eq!(scores.risk_logits.len(), 34);
            assert!(scores.value.is_finite());
        }
    }

    #[test]
    fn v2_masking_never_selects_illegal_discard() {
        let mut context = base_context();
        context.player.concealed_tiles = vec![tile("w1#0", "w1"), tile("t1#0", "t1")];
        context.restricted_discard_tile_key = Some("w1".to_string());

        let mut logits = [0.0_f32; TILE_KIND_COUNT];
        logits[tile_index("w1").expect("w1 index")] = 100.0;
        logits[tile_index("t1").expect("t1 index")] = 1.0;
        let ranked = rank_masked_discards(&context, &logits);

        assert_eq!(ranked[0].tile_key, "t1");
    }

    #[test]
    fn ranked_discards_follow_backend_tile_order_without_remap() {
        let mut context = base_context();
        context.player.concealed_tiles = vec![tile("b1#0", "b1"), tile("t1#0", "t1")];

        let mut logits = [0.0_f32; TILE_KIND_COUNT];
        logits[tile_index("b1").expect("b1 index")] = 4.0;
        logits[tile_index("t1").expect("t1 index")] = 2.0;
        let ranked = rank_masked_discards(&context, &logits);

        assert_eq!(ranked[0].tile_key, "b1");
    }

    #[test]
    fn claim_mask_ranking_filters_illegal_actions() {
        let mut context = base_context();
        context.claim_options = vec![crate::projection::bot_view::BotClaimOption {
            action_type: "pung".to_string(),
            tile_ids: vec!["w1#0".to_string(), "w1#1".to_string()],
        }];

        let mut logits = [0.0_f32; CLAIM_ACTION_COUNT];
        logits[claim_action_index("kong").expect("kong index")] = 100.0;
        logits[claim_action_index("pung").expect("pung index")] = 2.0;
        let ranked = rank_masked_claims(&context, &logits);

        assert_eq!(ranked[0].action_name, "pung");
    }

    #[test]
    fn runs_local_onnx_model_when_available() {
        let model_path = model_path_from_env();
        if !model_path.exists() {
            return;
        }
        if !local_model_manifest_is_sequence_aware(&model_path) {
            return;
        }
        let scores = OrtNeuralSession::new(model_path)
            .run(encode_bot_context_v2(&base_context()))
            .expect("local ONNX model should run");

        assert_eq!(scores.discard_logits.len(), TILE_KIND_COUNT);
        assert_eq!(scores.claim_logits.len(), CLAIM_ACTION_COUNT);
        assert_eq!(scores.self_kong_logits.len(), SELF_KONG_ACTION_COUNT);
        assert_eq!(scores.hu_logits.len(), 2);
        assert_eq!(scores.risk_logits.len(), TILE_KIND_COUNT);
    }

    #[test]
    fn explicit_model_path_reuses_thread_local_session_after_load_failure() {
        let model_path = PathBuf::from("missing-model-for-cache-test.onnx");
        CACHED_SESSIONS.with(|sessions| sessions.borrow_mut().clear());

        let context = base_context();
        assert!(neural_decision_scores_for_model_path(&context, Some(&model_path)).is_none());
        assert!(neural_decision_scores_for_model_path(&context, Some(&model_path)).is_none());

        CACHED_SESSIONS.with(|sessions| {
            let sessions = sessions.borrow();
            let session = sessions
                .get(&model_path)
                .expect("session cached by path")
                .borrow();
            assert_eq!(session.load_attempts, 1);
            assert!(session.disabled);
        });
    }

    #[test]
    fn explicit_backend_asset_path_resolves_to_runtime_asset_path_when_available() {
        let path = PathBuf::from("backend/assets/sft/sft.onnx");
        let resolved = resolve_model_path(&path);
        if Path::new("assets/sft/sft.onnx").exists() {
            assert_eq!(resolved, PathBuf::from("assets/sft/sft.onnx"));
        } else if Path::new("/app/assets/sft/sft.onnx").exists() {
            assert_eq!(resolved, PathBuf::from("/app/assets/sft/sft.onnx"));
        } else {
            assert_eq!(resolved, path);
        }
    }

    fn local_model_manifest_is_sequence_aware(model_path: &Path) -> bool {
        let Some(file_name) = model_path.file_name().and_then(|value| value.to_str()) else {
            return false;
        };
        let manifest_path = model_path.with_file_name(format!("{file_name}.manifest.json"));
        fs::read_to_string(manifest_path)
            .map(|content| {
                content.contains("discard_sequence_length")
                    && content.contains("discard_event_feature_count")
            })
            .unwrap_or(false)
    }
}
