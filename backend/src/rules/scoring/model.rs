pub use super::evaluator::{
    Decomposition, EvaluationInput, FanBreakdownEntry, FanResult, HandFeatures, KongEntry,
    KongScoreDetailEntry, ScoreDelta, TimingFeatures,
};

#[allow(dead_code)]
pub type ScoreRequest = EvaluationInput;
#[allow(dead_code)]
pub type ScoreResult = FanResult;
