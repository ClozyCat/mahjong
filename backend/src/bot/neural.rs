use super::context::{BotContext, BotTileView, TILE_KIND_COUNT, tile_index};
use ort::{session::Session, value::Tensor};
use std::{
    env,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const TRAINER_FEATURE_LAYERS: usize = 11;
const TRAINER_FEATURE_LEN: usize = TRAINER_FEATURE_LAYERS * TILE_KIND_COUNT;
const DEFAULT_MODEL_PATHS: [&str; 3] = [
    "assets/models/mahjong_policy_net.onnx",
    "backend/assets/models/mahjong_policy_net.onnx",
    "mahjong_policy_net.onnx",
];
const POLICY_ENV: &str = "MAHJONG_BOT_POLICY";
const MODEL_PATH_ENV: &str = "MAHJONG_BOT_MODEL_PATH";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NeuralDiscardScore {
    pub(crate) tile_id: String,
    pub(crate) tile_key: String,
    pub(crate) logit: f32,
}

pub(crate) fn neural_discard_scores(context: &BotContext) -> Option<Vec<NeuralDiscardScore>> {
    if !neural_policy_enabled() {
        return None;
    }
    let features = build_trainer_features(context);
    let logits = shared_session().lock().ok()?.run(features).ok()?;
    Some(rank_legal_discards_from_logits(context, &logits))
}

fn neural_policy_enabled() -> bool {
    env::var(POLICY_ENV)
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("hybrid"))
}

fn shared_session() -> &'static Mutex<OrtNeuralSession> {
    static SESSION: OnceLock<Mutex<OrtNeuralSession>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(OrtNeuralSession::new(model_path_from_env())))
}

fn model_path_from_env() -> PathBuf {
    if let Some(path) = env::var_os(MODEL_PATH_ENV) {
        return PathBuf::from(path);
    }
    DEFAULT_MODEL_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_PATHS[0]))
}

struct OrtNeuralSession {
    model_path: PathBuf,
    session: Option<Session>,
    disabled: bool,
}

impl OrtNeuralSession {
    fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            session: None,
            disabled: false,
        }
    }

    fn run(&mut self, features: Vec<f32>) -> Result<[f32; TILE_KIND_COUNT], ()> {
        if self.disabled {
            return Err(());
        }
        if self.session.is_none() {
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

fn load_session(model_path: &Path) -> Result<Session, ()> {
    Session::builder()
        .map_err(|_| ())?
        .commit_from_file(model_path)
        .map_err(|_| ())
}

fn run_session(session: &mut Session, features: Vec<f32>) -> Result<[f32; TILE_KIND_COUNT], ()> {
    let input = Tensor::from_array(([1_usize, TRAINER_FEATURE_LAYERS, TILE_KIND_COUNT], features))
        .map_err(|_| ())?;
    let outputs = session
        .run(ort::inputs!["input" => input])
        .map_err(|_| ())?;
    let output = if outputs.contains_key("output") {
        &outputs["output"]
    } else {
        &outputs[0]
    };
    let (_, values) = output.try_extract_tensor::<f32>().map_err(|_| ())?;
    let logits: [f32; TILE_KIND_COUNT] = values
        .get(0..TILE_KIND_COUNT)
        .ok_or(())?
        .try_into()
        .map_err(|_| ())?;
    Ok(logits)
}

fn build_trainer_features(context: &BotContext) -> Vec<f32> {
    let mut features = vec![0.0_f32; TRAINER_FEATURE_LEN];
    let mut hand_counts = [0_usize; TILE_KIND_COUNT];
    for tile in &context.player.concealed_tiles {
        if tile.is_flower {
            continue;
        }
        let Some(backend_index) = tile_index(&tile.tile_key) else {
            continue;
        };
        let Some(trainer_index) = trainer_index_for_backend_index(backend_index) else {
            continue;
        };
        let layer = hand_counts[backend_index];
        if layer < 4 {
            features[layer_offset(layer, trainer_index)] = 1.0;
        }
        hand_counts[backend_index] += 1;
    }

    set_tile_groups(&mut features, 4, &context.player.meld_tile_key_groups);

    for offset in 1..4 {
        let target_player = (context.seat_index + offset) % context.seat_count.max(1);
        let discard_layer = 4 + offset * 2 - 1;
        let meld_layer = 4 + offset * 2;
        if let Some(discards) = context.opponent_discards_by_seat.get(target_player) {
            set_tile_keys(
                &mut features,
                discard_layer,
                discards.iter().map(String::as_str),
            );
        }
        if let Some(melds) = context.opponent_melds_by_seat.get(target_player) {
            set_tile_groups(&mut features, meld_layer, melds);
        }
    }

    features
}

fn set_tile_groups(features: &mut [f32], layer: usize, groups: &[Vec<String>]) {
    set_tile_keys(
        features,
        layer,
        groups
            .iter()
            .flat_map(|group| group.iter().map(String::as_str)),
    );
}

fn set_tile_keys<'a>(features: &mut [f32], layer: usize, tile_keys: impl Iterator<Item = &'a str>) {
    for tile_key in tile_keys {
        let Some(trainer_index) = trainer_index_for_tile_key(tile_key) else {
            continue;
        };
        features[layer_offset(layer, trainer_index)] = 1.0;
    }
}

fn rank_legal_discards_from_logits(
    context: &BotContext,
    logits: &[f32; TILE_KIND_COUNT],
) -> Vec<NeuralDiscardScore> {
    let mut scores = Vec::new();
    let mut visited_tile_keys = std::collections::HashSet::new();
    for tile in &context.player.concealed_tiles {
        if tile.is_flower
            || Some(tile.tile_key.as_str()) == context.restricted_discard_tile_key.as_deref()
            || !visited_tile_keys.insert(tile.tile_key.clone())
        {
            continue;
        }
        let Some(trainer_index) = trainer_index_for_tile_key(&tile.tile_key) else {
            continue;
        };
        let Some(tile_id) =
            preferred_discard_tile_id_for_key(&context.player.concealed_tiles, &tile.tile_key)
        else {
            continue;
        };
        scores.push(NeuralDiscardScore {
            tile_id,
            tile_key: tile.tile_key.clone(),
            logit: logits[trainer_index],
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

fn preferred_discard_tile_id_for_key(
    concealed_tiles: &[BotTileView],
    tile_key: &str,
) -> Option<String> {
    concealed_tiles
        .iter()
        .find(|tile| !tile.is_flower && tile.tile_key == tile_key)
        .map(|tile| tile.tile_id.clone())
}

fn trainer_index_for_tile_key(tile_key: &str) -> Option<usize> {
    trainer_index_for_backend_index(tile_index(tile_key)?)
}

fn trainer_index_for_backend_index(backend_index: usize) -> Option<usize> {
    match backend_index {
        0..=8 => Some(backend_index),
        9..=17 => Some(backend_index + 9),
        18..=26 => Some(backend_index - 9),
        27..=33 => Some(backend_index),
        _ => None,
    }
}

fn layer_offset(layer: usize, trainer_index: usize) -> usize {
    layer * TILE_KIND_COUNT + trainer_index
}

#[cfg(test)]
fn trainer_offset(layer: usize, tile_key: &str) -> usize {
    layer_offset(
        layer,
        trainer_index_for_tile_key(tile_key).expect("trainer tile index"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::bot_view::{BotContextView, BotPlayerView, BotTileView};
    use std::collections::HashSet;

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
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: 42,
            visible_tile_keys: Vec::new(),
            opponent_discards_by_seat: vec![vec![], vec![], vec![], vec![]],
            opponent_melds_by_seat: vec![vec![], vec![], vec![], vec![]],
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
    fn maps_backend_tile_order_to_trainer_tile_order() {
        assert_eq!(trainer_index_for_backend_index(0), Some(0));
        assert_eq!(trainer_index_for_backend_index(8), Some(8));
        assert_eq!(trainer_index_for_backend_index(9), Some(18));
        assert_eq!(trainer_index_for_backend_index(17), Some(26));
        assert_eq!(trainer_index_for_backend_index(18), Some(9));
        assert_eq!(trainer_index_for_backend_index(26), Some(17));
        assert_eq!(trainer_index_for_backend_index(27), Some(27));
        assert_eq!(trainer_index_for_backend_index(30), Some(30));
        assert_eq!(trainer_index_for_backend_index(33), Some(33));
        assert_eq!(trainer_index_for_backend_index(34), None);
    }

    #[test]
    fn builds_trainer_feature_layers_from_bot_context() {
        let mut context = base_context();
        context.seat_index = 1;
        context.player.concealed_tiles = vec![
            tile("w1#0", "w1"),
            tile("w1#1", "w1"),
            tile("t1#0", "t1"),
            tile("b9#0", "b9"),
        ];
        context.player.meld_tile_key_groups = vec![vec![
            "red".to_string(),
            "red".to_string(),
            "red".to_string(),
        ]];
        context.opponent_discards_by_seat[2] = vec!["b1".to_string()];
        context.opponent_melds_by_seat[2] =
            vec![vec!["t2".to_string(), "t3".to_string(), "t4".to_string()]];
        context.opponent_discards_by_seat[3] = vec!["east".to_string()];
        context.opponent_melds_by_seat[0] = vec![vec![
            "white".to_string(),
            "white".to_string(),
            "white".to_string(),
        ]];

        let features = build_trainer_features(&context);

        assert_eq!(features[trainer_offset(0, "w1")], 1.0);
        assert_eq!(features[trainer_offset(1, "w1")], 1.0);
        assert_eq!(features[trainer_offset(2, "w1")], 0.0);
        assert_eq!(features[trainer_offset(0, "t1")], 1.0);
        assert_eq!(features[trainer_offset(0, "b9")], 1.0);
        assert_eq!(features[trainer_offset(4, "red")], 1.0);
        assert_eq!(features[trainer_offset(5, "b1")], 1.0);
        assert_eq!(features[trainer_offset(6, "t3")], 1.0);
        assert_eq!(features[trainer_offset(7, "east")], 1.0);
        assert_eq!(features[trainer_offset(10, "white")], 1.0);
    }

    #[test]
    fn ranks_logits_by_legal_discard_tiles_only() {
        let mut context = base_context();
        context.player.concealed_tiles =
            vec![tile("w1#0", "w1"), tile("t1#0", "t1"), tile("b1#0", "b1")];
        context.restricted_discard_tile_key = Some("b1".to_string());
        let mut logits = [0.0_f32; 34];
        logits[trainer_index_for_tile_key("b1").expect("b1 trainer index")] = 99.0;
        logits[trainer_index_for_tile_key("t1").expect("t1 trainer index")] = 8.0;
        logits[trainer_index_for_tile_key("w1").expect("w1 trainer index")] = 4.0;

        let ranked = rank_legal_discards_from_logits(&context, &logits);

        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].tile_key, "t1");
        assert_eq!(ranked[0].tile_id, "t1#0");
        assert_eq!(ranked[1].tile_key, "w1");
    }

    #[test]
    fn runs_local_onnx_model_when_available() {
        let model_path = model_path_from_env();
        if !model_path.exists() {
            return;
        }
        let logits = OrtNeuralSession::new(model_path)
            .run(vec![0.0; TRAINER_FEATURE_LEN])
            .expect("local ONNX model should run");

        assert_eq!(logits.len(), TILE_KIND_COUNT);
        assert!(logits.iter().all(|logit| logit.is_finite()));
    }
}
