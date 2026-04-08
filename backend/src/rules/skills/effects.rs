pub use crate::core::state::effect::{EffectInstance, EffectState, KnowledgeEffect, RuleOverride};

use crate::core::ids::Seat;

#[allow(dead_code)]
pub fn visible_effects_for_seat<'a>(
    effect_state: &'a EffectState,
    seat: Seat,
) -> Vec<&'a EffectInstance> {
    effect_state
        .ongoing
        .iter()
        .filter(|effect| effect.owner == seat || effect.target_seats.contains(&seat))
        .collect()
}
