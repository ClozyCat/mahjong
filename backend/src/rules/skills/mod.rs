mod builtin;
pub mod catalog;
pub mod draft;
pub mod effects;
pub mod hooks;
pub mod instances;
pub mod registry;
mod strategems;

use std::sync::OnceLock;

use serde::Serialize;
use serde_json::{Value, json};

use crate::core::event::GameEvent;
use crate::core::ids::{Seat, SkillId, TileId};
use crate::core::state::{
    LastActionContext, PendingAction, PendingTimeout, RoomState, RoundSettlement, SkillDraftStatus,
    SkillRarity as DraftSkillRarity,
};
use crate::core::tile::Tile;
use crate::room_scoring::RoomScoringCache;
use crate::rules::standard::runtime::sync_pending_timeout_in_room_state;
use crate::rules::standard::win::can_declare_hu_with_cache_for_state;

use self::builtin::{PeekOpponentTileSkill, ScoreBoostSkill};
use self::catalog::{
    SkillInteractionKind as CatalogInteractionKind, SkillKind, SkillRarity as CatalogSkillRarity,
    active_uses_per_round_for_skill, catalog as loaded_skill_catalog, detail_for_skill,
    entry as catalog_entry, interaction_hint_for_skill,
    interaction_kind_for_skill as catalog_interaction_kind_for_skill, kind_for_skill,
    rarity_for_level,
};

#[allow(unused_imports)]
pub use effects::{
    EffectInstance, EffectState, KnowledgeEffect, RuleOverride, visible_effects_for_seat,
};
#[allow(unused_imports)]
pub use hooks::{
    DrawRequest, HuCheckRequest, RuleContext, RuleHook, ScoreHookRequest, SkillActivation,
    SkillContext, SkillHookKind, SkillProjection,
};
#[allow(unused_imports)]
pub use instances::{
    SkillInstance, SkillLoadout, find_skill_instance, seat_skill_loadout, seat_skill_state,
};
pub use registry::{SkillDefinition, SkillRegistry, StaticSkillRegistry};

pub fn default_registry() -> &'static StaticSkillRegistry {
    static REGISTRY: OnceLock<StaticSkillRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = StaticSkillRegistry::new();
        registry.register(std::sync::Arc::new(ScoreBoostSkill));
        registry.register(std::sync::Arc::new(PeekOpponentTileSkill));
        for definition in strategems::definitions() {
            registry.register(definition);
        }
        registry
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EquippedSkillView {
    pub skill_id: SkillId,
    pub serial: Option<String>,
    pub name: String,
    pub rarity: String,
    pub rarity_label: String,
    pub tone: String,
    #[serde(rename = "type")]
    pub skill_type: String,
    pub type_label: String,
    pub interaction_kind: Option<String>,
    pub summary: String,
    pub detail: String,
    pub interaction_hint: Option<String>,
    pub tags: Vec<String>,
    pub remaining_rounds: u8,
    pub remaining_activations_this_round: u8,
    pub can_activate_now: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillSelectionView {
    pub cycle_key: String,
    pub cycle_label: String,
    pub deadline_at: String,
    pub title: String,
    pub detail: String,
    pub options: Vec<EquippedSkillView>,
}

fn skills_enabled(room_state: &RoomState) -> bool {
    room_state.mode == "skill"
}

pub fn equipped_skill_views(room_state: &RoomState, seat: Seat) -> Vec<EquippedSkillView> {
    equipped_skill_views_with_registry(room_state, seat, default_registry())
}

pub fn equipped_skill_views_with_registry(
    room_state: &RoomState,
    seat: Seat,
    registry: &dyn SkillRegistry,
) -> Vec<EquippedSkillView> {
    if !skills_enabled(room_state) {
        return Vec::new();
    }

    let Some(loadout) = seat_skill_loadout(room_state, seat) else {
        return Vec::new();
    };
    loadout
        .equipped
        .iter()
        .map(|skill_instance| equipped_skill_view(room_state, seat, skill_instance, registry))
        .collect()
}

pub fn public_skill_view(room_state: &RoomState, seat: Seat) -> Option<EquippedSkillView> {
    public_skill_view_with_registry(room_state, seat, default_registry())
}

pub fn public_skill_view_with_registry(
    room_state: &RoomState,
    seat: Seat,
    registry: &dyn SkillRegistry,
) -> Option<EquippedSkillView> {
    if !skills_enabled(room_state) {
        return None;
    }

    seat_skill_loadout(room_state, seat)
        .and_then(|loadout| loadout.equipped.first())
        .map(|skill_instance| equipped_skill_view(room_state, seat, skill_instance, registry))
}

pub fn current_skill_selection_view(
    room_state: &RoomState,
    seat: Seat,
) -> Option<SkillSelectionView> {
    current_skill_selection_view_with_registry(room_state, seat, default_registry())
}

pub fn current_skill_selection_view_with_registry(
    room_state: &RoomState,
    seat: Seat,
    registry: &dyn SkillRegistry,
) -> Option<SkillSelectionView> {
    if !skills_enabled(room_state) {
        return None;
    }

    let round = room_state.round_state.as_ref()?;
    let draft = round.skill_draft.as_ref()?;
    let offer = draft.offers_by_seat.get(&seat)?;
    if offer.status != SkillDraftStatus::Pending {
        return None;
    }

    let options = offer
        .options
        .iter()
        .map(|choice| offer_choice_view(room_state, seat, choice, &draft.cycle_label, registry))
        .collect();

    Some(SkillSelectionView {
        cycle_key: draft.cycle_key.clone(),
        cycle_label: draft.cycle_label.clone(),
        deadline_at: draft.deadline_at.clone(),
        title: format!("{} · 技能签启", draft.cycle_label),
        detail: "每种技能至多持续两局；主动技能每局可按技能品质发动对应次数，未用次数不会累加。"
            .to_string(),
        options,
    })
}

fn interaction_kind_for_skill(skill_id: &str) -> Option<&'static str> {
    match catalog_interaction_kind_for_skill(skill_id) {
        Some(CatalogInteractionKind::Confirm) => Some("confirm"),
        Some(CatalogInteractionKind::PreviewWall) => Some("preview_wall"),
        Some(CatalogInteractionKind::SelectTarget) => Some("select_target"),
        Some(CatalogInteractionKind::SelectHandTile) => Some("select_hand_tile"),
        Some(CatalogInteractionKind::SelectMeld) => Some("select_meld"),
        None => match skill_id {
            "peek_opponent_tile" => Some("select_target"),
            "score_boost" => Some("confirm"),
            _ => None,
        },
    }
}

fn equipped_skill_view(
    room_state: &RoomState,
    owner: Seat,
    skill_instance: &SkillInstance,
    registry: &dyn SkillRegistry,
) -> EquippedSkillView {
    let catalog_entry = catalog_entry(&skill_instance.skill_id);
    let name = catalog_entry
        .map(|entry| entry.name.clone())
        .or_else(|| {
            registry
                .get(&skill_instance.skill_id)
                .map(|definition| definition.name().to_string())
        })
        .unwrap_or_else(|| "Unknown Skill".to_string());
    let rarity = rarity_for_level(skill_instance.level);
    let skill_kind = kind_for_skill(&skill_instance.skill_id).unwrap_or_else(|| {
        registry
            .get(&skill_instance.skill_id)
            .map(|definition| match definition.activation() {
                SkillActivation::ActiveTurn => SkillKind::Active,
                SkillActivation::Passive => SkillKind::Passive,
            })
            .unwrap_or(SkillKind::Passive)
    });

    EquippedSkillView {
        skill_id: skill_instance.skill_id.clone(),
        serial: catalog_entry.and_then(|entry| entry.serial.clone()),
        name: name.clone(),
        rarity: skill_rarity_key(rarity).to_string(),
        rarity_label: skill_rarity_label(rarity).to_string(),
        tone: skill_rarity_tone(rarity).to_string(),
        skill_type: match skill_kind {
            SkillKind::Active => "active".to_string(),
            SkillKind::Passive => "passive".to_string(),
        },
        type_label: match skill_kind {
            SkillKind::Active => "主动技能".to_string(),
            SkillKind::Passive => "被动技能".to_string(),
        },
        interaction_kind: interaction_kind_for_skill(&skill_instance.skill_id)
            .map(ToString::to_string),
        summary: catalog_entry
            .map(|entry| entry.summary.clone())
            .unwrap_or_else(|| name.clone()),
        detail: detail_for_skill(&skill_instance.skill_id, skill_instance.level)
            .map(|detail| format!("{}效果：{detail}", skill_rarity_label(rarity)))
            .unwrap_or_default(),
        interaction_hint: interaction_hint_for_skill(&skill_instance.skill_id),
        tags: catalog_entry
            .map(|entry| entry.tags.clone())
            .unwrap_or_default(),
        remaining_rounds: remaining_rounds_for_skill(skill_instance),
        remaining_activations_this_round: skill_instance.charges,
        can_activate_now: can_activate_skill_now(room_state, owner, skill_instance, skill_kind),
    }
}

fn offer_choice_view(
    room_state: &RoomState,
    owner: Seat,
    choice: &crate::core::state::SkillDraftChoice,
    cycle_label: &str,
    registry: &dyn SkillRegistry,
) -> EquippedSkillView {
    let rarity = match choice.rarity {
        DraftSkillRarity::Common => CatalogSkillRarity::Common,
        DraftSkillRarity::Rare => CatalogSkillRarity::Rare,
        DraftSkillRarity::Epic => CatalogSkillRarity::Epic,
    };
    let level = choice.rarity.level();
    let skill_instance = SkillInstance {
        skill_id: choice.skill_id.clone(),
        owner,
        level,
        rarity: rarity.key().to_string(),
        remaining_rounds: 2,
        cooldown: 0,
        charges: active_uses_per_round_for_skill(&choice.skill_id, level),
        charges_per_round: active_uses_per_round_for_skill(&choice.skill_id, level),
        config: json!({
            "remaining_rounds": 2,
            "cycle_label": cycle_label,
            "rarity": rarity.key(),
        }),
    };
    let mut view = equipped_skill_view(room_state, owner, &skill_instance, registry);
    view.rarity = skill_rarity_key(rarity).to_string();
    view.rarity_label = skill_rarity_label(rarity).to_string();
    view.tone = skill_rarity_tone(rarity).to_string();
    view.remaining_rounds = 2;
    view.remaining_activations_this_round = skill_instance.charges;
    view.can_activate_now = false;
    view
}

fn remaining_rounds_for_skill(skill_instance: &SkillInstance) -> u8 {
    if skill_instance.remaining_rounds > 0 {
        return skill_instance.remaining_rounds;
    }
    skill_instance
        .config
        .get("remaining_rounds")
        .and_then(Value::as_u64)
        .map(|value| value as u8)
        .unwrap_or(0)
}

fn can_activate_skill_now(
    room_state: &RoomState,
    owner: Seat,
    skill_instance: &SkillInstance,
    skill_kind: SkillKind,
) -> bool {
    skill_kind == SkillKind::Active
        && room_state.phase == "playing"
        && room_state
            .round_state
            .as_ref()
            .map(|round| round.current_actor == owner)
            .unwrap_or(false)
        && skill_instance.charges > 0
}

fn skill_rarity_label(rarity: CatalogSkillRarity) -> &'static str {
    match rarity {
        CatalogSkillRarity::Common => "普通",
        CatalogSkillRarity::Rare => "稀有",
        CatalogSkillRarity::Epic => "史诗",
    }
}

fn skill_rarity_tone(rarity: CatalogSkillRarity) -> &'static str {
    match rarity {
        CatalogSkillRarity::Common => "jade",
        CatalogSkillRarity::Rare => "azure",
        CatalogSkillRarity::Epic => "violet",
    }
}

fn skill_rarity_key(rarity: CatalogSkillRarity) -> &'static str {
    match rarity {
        CatalogSkillRarity::Common => "common",
        CatalogSkillRarity::Rare => "rare",
        CatalogSkillRarity::Epic => "epic",
    }
}

pub fn begin_round_skill_draft_in_room_state(room: &mut RoomState) -> Result<(), String> {
    draft::begin_round_skill_draft_in_room_state(room)?;
    if room
        .round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .is_some_and(|draft| draft.is_active())
    {
        if let Some(round) = room.round_state.as_mut() {
            round.pending_action = None;
        }
        room.pending_timeout = Some(PendingTimeout {
            kind: "skill_draft".to_string(),
            seat_index: next_pending_skill_draft_seat(room)
                .or_else(|| room.round_state.as_ref().map(|round| round.current_actor))
                .unwrap_or(0),
            deadline_at: room
                .round_state
                .as_ref()
                .and_then(|round| round.skill_draft.as_ref())
                .map(|draft| draft.deadline_at.clone()),
            drawn_tile_id: None,
        });
    } else {
        restore_round_start_after_skill_draft_in_room_state(room)?;
    }
    Ok(())
}

pub fn advance_skill_loadouts_for_next_round_in_room_state(room: &mut RoomState) {
    draft::advance_skill_loadouts_for_next_round_in_room_state(room);
}

pub fn clear_skill_loadouts_for_new_match_in_room_state(room: &mut RoomState) {
    draft::clear_skill_loadouts_for_new_match_in_room_state(room);
}

pub fn resolve_due_skill_draft_in_room_state(room: &mut RoomState) -> Result<Vec<Value>, String> {
    let messages = draft::resolve_due_skill_draft_in_room_state(room)?;
    restore_round_start_after_skill_draft_in_room_state(room)?;
    Ok(messages)
}

pub fn select_skill_for_draft_in_room_state(
    room: &mut RoomState,
    seat: Seat,
    skill_id: &str,
) -> Result<crate::core::engine::EngineOutput, String> {
    draft::select_skill_offer_in_room_state(room, seat, skill_id)?;
    if draft::all_skill_draft_responses_complete(room) {
        restore_round_start_after_skill_draft_in_room_state(room)?;
    } else if let Some(deadline_at) = draft::next_skill_draft_deadline(room) {
        room.pending_timeout = Some(PendingTimeout {
            kind: "skill_draft".to_string(),
            seat_index: next_pending_skill_draft_seat(room).unwrap_or(seat),
            deadline_at: Some(deadline_at.to_string()),
            drawn_tile_id: None,
        });
    }
    Ok(crate::core::engine::EngineOutput::default())
}

pub fn decline_skill_draft_in_room_state(
    room: &mut RoomState,
    seat: Seat,
) -> Result<crate::core::engine::EngineOutput, String> {
    draft::decline_skill_offer_in_room_state(room, seat)?;
    if draft::all_skill_draft_responses_complete(room) {
        restore_round_start_after_skill_draft_in_room_state(room)?;
    } else if let Some(deadline_at) = draft::next_skill_draft_deadline(room) {
        room.pending_timeout = Some(PendingTimeout {
            kind: "skill_draft".to_string(),
            seat_index: next_pending_skill_draft_seat(room).unwrap_or(seat),
            deadline_at: Some(deadline_at.to_string()),
            drawn_tile_id: None,
        });
    }
    Ok(crate::core::engine::EngineOutput::default())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillDraftSelectionView {
    pub cycle_key: String,
    pub cycle_label: String,
    pub deadline_at: String,
    pub title: String,
    pub detail: String,
    pub options: Vec<SkillDraftChoiceView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillDraftChoiceView {
    pub skill_id: SkillId,
    pub serial: Option<String>,
    pub name: String,
    pub rarity: String,
    pub rarity_label: String,
    pub tone: String,
    #[serde(rename = "type")]
    pub skill_type: String,
    pub type_label: String,
    pub interaction_kind: Option<String>,
    pub summary: String,
    pub detail: String,
    pub interaction_hint: Option<String>,
    pub tags: Vec<String>,
    pub remaining_rounds: u8,
    pub remaining_activations_this_round: u8,
}

pub fn skill_draft_view(room_state: &RoomState, seat: Seat) -> Option<SkillDraftSelectionView> {
    if !skills_enabled(room_state) {
        return None;
    }

    let round = room_state.round_state.as_ref()?;
    let draft = round.skill_draft.as_ref()?;
    let offer = draft.offers_by_seat.get(&seat)?;
    if offer.status != crate::core::state::SkillDraftStatus::Pending {
        return None;
    }
    Some(SkillDraftSelectionView {
        cycle_key: draft.cycle_key.clone(),
        cycle_label: draft.cycle_label.clone(),
        deadline_at: draft.deadline_at.clone(),
        title: format!("{} · 技能签启", draft.cycle_label),
        detail: "每种技能持续两局；主动技能每局可按技能品质发动对应次数，未使用次数不会累加。"
            .to_string(),
        options: offer
            .options
            .iter()
            .filter_map(|choice| {
                let entry = catalog_entry(&choice.skill_id)?;
                let rarity = match choice.rarity {
                    crate::core::state::SkillRarity::Common => CatalogSkillRarity::Common,
                    crate::core::state::SkillRarity::Rare => CatalogSkillRarity::Rare,
                    crate::core::state::SkillRarity::Epic => CatalogSkillRarity::Epic,
                };
                Some(SkillDraftChoiceView {
                    skill_id: choice.skill_id.clone(),
                    serial: entry.serial.clone(),
                    name: entry.name.clone(),
                    rarity: match choice.rarity {
                        crate::core::state::SkillRarity::Common => "common".to_string(),
                        crate::core::state::SkillRarity::Rare => "rare".to_string(),
                        crate::core::state::SkillRarity::Epic => "epic".to_string(),
                    },
                    rarity_label: choice.rarity.label().to_string(),
                    tone: choice.rarity.tone().to_string(),
                    skill_type: match entry.skill_type {
                        SkillKind::Active => "active".to_string(),
                        SkillKind::Passive => "passive".to_string(),
                    },
                    type_label: entry.skill_type.label().to_string(),
                    interaction_kind: entry.interaction_kind.map(|kind| match kind {
                        CatalogInteractionKind::Confirm => "confirm".to_string(),
                        CatalogInteractionKind::PreviewWall => "preview_wall".to_string(),
                        CatalogInteractionKind::SelectTarget => "select_target".to_string(),
                        CatalogInteractionKind::SelectHandTile => "select_hand_tile".to_string(),
                        CatalogInteractionKind::SelectMeld => "select_meld".to_string(),
                    }),
                    summary: entry.summary.clone(),
                    detail: format!(
                        "{}效果：{}",
                        choice.rarity.label(),
                        entry.tier(rarity).detail
                    ),
                    interaction_hint: entry.interaction_hint.clone(),
                    tags: entry.tags.clone(),
                    remaining_rounds: loaded_skill_catalog().selection.duration_rounds,
                    remaining_activations_this_round: active_uses_per_round_for_skill(
                        &choice.skill_id,
                        choice.rarity.level(),
                    ),
                })
            })
            .collect(),
    })
}

fn restore_round_start_after_skill_draft_in_room_state(room: &mut RoomState) -> Result<(), String> {
    draft::clear_skill_draft_in_room_state(room);
    let round = room
        .round_state
        .as_mut()
        .ok_or_else(|| "round_not_ready".to_string())?;
    let dealer_seat = round.dealer_seat;
    let dealer_drawn_tile_id = round.last_action_context.tile_id.clone();
    let dealer_first_flower_id = round
        .players
        .get(dealer_seat)
        .and_then(|player| {
            player
                .concealed_tiles
                .iter()
                .find(|tile| tile.kind == "flower")
        })
        .map(|tile| tile.tile_id.clone());
    let has_any_flower = round.players.iter().any(|player| {
        player
            .concealed_tiles
            .iter()
            .any(|tile| tile.kind == "flower")
    });
    round.pending_action = if has_any_flower {
        round.score_trackers.opening_flowers_completed = false;
        Some(PendingAction::OpeningFlowers(
            crate::core::state::OpeningFlowersAction { dealer_seat },
        ))
    } else {
        round.score_trackers.opening_flowers_completed = true;
        None
    };
    room.pending_timeout = Some(PendingTimeout {
        kind: if has_any_flower {
            "opening_flowers".to_string()
        } else {
            "active_turn".to_string()
        },
        seat_index: dealer_seat,
        deadline_at: None,
        drawn_tile_id: if has_any_flower {
            dealer_first_flower_id
        } else {
            dealer_drawn_tile_id
        },
    });
    sync_pending_timeout_in_room_state(room);
    Ok(())
}

fn next_pending_skill_draft_seat(room: &RoomState) -> Option<Seat> {
    room.round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .and_then(|draft| {
            draft.offers_by_seat.iter().find_map(|(seat, offer)| {
                (offer.status == SkillDraftStatus::Pending).then_some(*seat)
            })
        })
}

fn update_skill_round_state_in_room_state<F>(
    room: &mut RoomState,
    mut mutate: F,
) -> Result<(), String>
where
    F: FnMut(&mut crate::core::state::RoundState) -> Result<(), String>,
{
    let round = room
        .round_state
        .as_mut()
        .ok_or_else(|| "invalid_action".to_string())?;
    mutate(round)
}

fn update_skill_match_state_in_room_state<F>(
    room: &mut RoomState,
    mut mutate: F,
) -> Result<(), String>
where
    F: FnMut(&mut crate::core::state::MatchState) -> Result<(), String>,
{
    let match_state = room
        .match_state
        .as_mut()
        .ok_or_else(|| "invalid_action".to_string())?;
    mutate(match_state)
}

pub fn activate_skill(
    room_state: &RoomState,
    actor: Seat,
    skill_id: &str,
    target: Option<Seat>,
    tile_ids: &[TileId],
) -> Result<Vec<GameEvent>, String> {
    activate_skill_with_registry(
        room_state,
        actor,
        skill_id,
        target,
        tile_ids,
        default_registry(),
    )
}

pub fn activate_skill_with_registry(
    room_state: &RoomState,
    actor: Seat,
    skill_id: &str,
    target: Option<Seat>,
    tile_ids: &[TileId],
    registry: &dyn SkillRegistry,
) -> Result<Vec<GameEvent>, String> {
    if !skills_enabled(room_state) {
        return Err("invalid_action".to_string());
    }

    if room_state.phase != "playing" {
        return Err("invalid_action".to_string());
    }
    if room_state
        .round_state
        .as_ref()
        .map(|round| round.current_actor != actor)
        .unwrap_or(true)
    {
        return Err("invalid_action".to_string());
    }
    let skill_instance = find_skill_instance(room_state, actor, &skill_id.to_string())
        .ok_or_else(|| "skill_not_equipped".to_string())?;
    if skill_instance.owner != actor {
        return Err("invalid_action".to_string());
    }
    if skill_instance.charges == 0 {
        return Err("skill_no_charges".to_string());
    }
    let definition = registry
        .get(skill_id)
        .ok_or_else(|| "skill_not_registered".to_string())?;
    let mut ctx = SkillContext::new(room_state, actor, skill_instance);
    definition.can_activate(&ctx, target, tile_ids)?;
    definition.activate(&mut ctx, target, tile_ids)
}

#[allow(dead_code)]
pub fn has_registered_skill(registry: &dyn SkillRegistry, skill_id: &SkillId) -> bool {
    registry.get(skill_id).is_some()
}

pub fn skill_action_options(room_state: &RoomState, seat: Seat) -> Vec<String> {
    skill_action_options_with_registry(room_state, seat, default_registry())
}

pub fn skill_action_options_with_registry(
    room_state: &RoomState,
    seat: Seat,
    registry: &dyn SkillRegistry,
) -> Vec<String> {
    if !skills_enabled(room_state) {
        return Vec::new();
    }

    let mut options = Vec::new();
    let _ = for_each_equipped_skill(room_state, registry, |ctx, definition| {
        if ctx.actor == seat {
            definition.append_action_options(ctx, &mut options)?;
        }
        Ok(())
    });
    options.sort();
    options.dedup();
    options
}

pub fn build_skill_projection(room_state: &RoomState, local_seat: Seat) -> SkillProjection {
    build_skill_projection_with_registry(room_state, local_seat, default_registry())
}

pub fn build_skill_projection_with_registry(
    room_state: &RoomState,
    local_seat: Seat,
    registry: &dyn SkillRegistry,
) -> SkillProjection {
    if !skills_enabled(room_state) {
        return SkillProjection::default();
    }

    let mut projection = SkillProjection::default();
    let _ = for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.build_view(ctx, local_seat, &mut projection)
    });
    dedup_projection(&mut projection);
    projection
}

pub fn apply_before_scoring_hooks(
    room_state: &RoomState,
    request: &mut ScoreHookRequest,
) -> Result<(), String> {
    apply_before_scoring_hooks_with_registry(room_state, request, default_registry())
}

pub fn apply_before_scoring_hooks_with_registry(
    room_state: &RoomState,
    request: &mut ScoreHookRequest,
    registry: &dyn SkillRegistry,
) -> Result<(), String> {
    if !skills_enabled(room_state) {
        return Ok(());
    }

    for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.before_scoring(ctx, request)
    })
}

pub fn apply_after_scoring_hooks(
    room_state: &RoomState,
    request: &ScoreHookRequest,
    result: &mut crate::rules::scoring::FanResult,
) -> Result<(), String> {
    apply_after_scoring_hooks_with_registry(room_state, request, result, default_registry())
}

pub fn apply_after_scoring_hooks_with_registry(
    room_state: &RoomState,
    request: &ScoreHookRequest,
    result: &mut crate::rules::scoring::FanResult,
    registry: &dyn SkillRegistry,
) -> Result<(), String> {
    if !skills_enabled(room_state) {
        return Ok(());
    }

    for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.after_scoring(ctx, request, result)
    })
}

pub fn apply_draw_settlement_hooks(
    room_state: &RoomState,
    settlement: &mut RoundSettlement,
) -> Result<(), String> {
    apply_draw_settlement_hooks_with_registry(room_state, settlement, default_registry())
}

pub fn apply_draw_settlement_hooks_with_registry(
    room_state: &RoomState,
    settlement: &mut RoundSettlement,
    registry: &dyn SkillRegistry,
) -> Result<(), String> {
    if !skills_enabled(room_state) {
        return Ok(());
    }

    for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.after_draw_settlement(ctx, settlement)
    })
}

pub fn apply_skill_events_to_room_in_room_state(
    room: &mut RoomState,
    actor: Seat,
    skill_id: &str,
    events: &[GameEvent],
) -> Result<Vec<Value>, String> {
    decrement_skill_charge_in_room_state(room, actor, skill_id)?;
    let emitted = apply_events_to_room_in_room_state(room, events)?;
    sync_round_skill_trackers_in_room_state(room);
    Ok(emitted)
}

pub fn apply_passive_skill_events_to_room_in_room_state(
    room: &mut RoomState,
    events: &[GameEvent],
) -> Result<Vec<Value>, String> {
    let emitted = apply_events_to_room_in_room_state(room, events)?;
    sync_round_skill_trackers_in_room_state(room);
    Ok(emitted)
}

pub fn sync_match_skill_trackers_after_settlement_in_room_state(room: &mut RoomState) {
    let winner_seat = room
        .round_state
        .as_ref()
        .and_then(|round| round.settlement.as_ref())
        .and_then(|settlement| settlement.winner_seat);

    let seat_count = room.seats.len().max(4);
    let mut trackers = room
        .match_state
        .as_ref()
        .map(|match_state| match_state.skill_trackers.clone())
        .unwrap_or_default();
    if let Some(winner) = winner_seat {
        trackers
            .zou_wei_shang_ji
            .pending_win_penalty
            .remove(&winner);
    }
    match winner_seat {
        Some(winner) => {
            for seat in 0..seat_count {
                let next = if seat == winner {
                    trackers
                        .lian_huan_ji
                        .streaks
                        .get(&seat)
                        .copied()
                        .unwrap_or(0)
                        + 1
                } else {
                    0
                };
                trackers.lian_huan_ji.streaks.insert(seat, next);
            }
        }
        None => {
            for seat in 0..seat_count {
                trackers.lian_huan_ji.streaks.insert(seat, 0);
            }
        }
    }

    let _ = update_skill_match_state_in_room_state(room, |match_state| {
        match_state.skill_trackers = trackers.clone();
        Ok(())
    });
}

pub fn sync_round_skill_trackers_in_room_state(room: &mut RoomState) {
    let Some(round) = room.round_state.as_ref() else {
        return;
    };
    let seat_count = round.players.len();
    let mut trackers = round.skill_trackers.clone();

    let mut discard_counts = std::collections::BTreeMap::new();
    let mut discarded_five_by_seat = std::collections::BTreeMap::new();
    let mut discard_suits_by_seat = std::collections::BTreeMap::new();
    let mut players_with_kong = Vec::new();

    for (seat, player) in round.players.iter().enumerate() {
        let mut suit_set = std::collections::BTreeSet::new();
        let mut discarded_five = false;
        for discard in &player.discards {
            let tile_key = discard.tile_key.as_str();
            *discard_counts.entry(tile_key.to_string()).or_default() += 1;
            if is_suit_five(tile_key) {
                discarded_five = true;
            }
            if let Some(prefix) = suit_prefix(tile_key) {
                suit_set.insert(prefix.to_string());
            }
        }
        discarded_five_by_seat.insert(seat, discarded_five);
        discard_suits_by_seat.insert(seat, suit_set.into_iter().collect());
        let has_kong = player.melds.iter().any(|meld| meld.len() == 4);
        if has_kong {
            players_with_kong.push(seat);
        }
    }
    trackers.discard_counts = discard_counts;
    trackers.discarded_five_by_seat = discarded_five_by_seat;
    trackers.discard_suits_by_seat = discard_suits_by_seat;
    trackers.players_with_kong = players_with_kong;
    trackers.live_tiles_remaining = round.wall.live_tiles_remaining() as i64;
    trackers.tiles_drawn_since_opening = round.wall.head_index.saturating_sub(53) as i64;
    trackers.multi_hu_candidates = pending_multi_hu_candidates(round);

    let (tenpai_seats, tenpai_waits_by_seat) =
        compute_tenpai_trackers_in_room_state(room, seat_count);
    trackers.tenpai_seats = tenpai_seats;
    trackers.tenpai_waits_by_seat = tenpai_waits_by_seat;

    let _ = update_skill_round_state_in_room_state(room, |round| {
        round.skill_trackers = trackers.clone();
        Ok(())
    });
}

pub fn note_tracker_discard_in_room_state(room: &mut RoomState, seat: Seat, tile_key: &str) {
    let mut trackers = room
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.clone())
        .unwrap_or_default();
    if is_honor_tile_key(tile_key) {
        trackers
            .pending_honor_rebuy_tile_by_seat
            .insert(seat, tile_key.to_string());
    } else {
        trackers.pending_honor_rebuy_tile_by_seat.remove(&seat);
    }
    let _ = update_skill_round_state_in_room_state(room, |round| {
        round.skill_trackers = trackers.clone();
        Ok(())
    });
}

pub fn note_tracker_draw_in_room_state(room: &mut RoomState, seat: Seat, tile_key: &str) {
    let pending_tile = room
        .round_state
        .as_ref()
        .and_then(|round| {
            round
                .skill_trackers
                .pending_honor_rebuy_tile_by_seat
                .get(&seat)
        })
        .map(ToString::to_string);
    let mut trackers = room
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.clone())
        .unwrap_or_default();
    trackers.pending_honor_rebuy_tile_by_seat.remove(&seat);
    if pending_tile.as_deref() == Some(tile_key) {
        trackers.honor_redraw_success_by_seat.insert(seat, true);
    }
    let _ = update_skill_round_state_in_room_state(room, |round| {
        round.skill_trackers = trackers.clone();
        Ok(())
    });
}

pub fn note_tracker_claimed_discard_in_room_state(room: &mut RoomState, discarder_seat: Seat) {
    let mut trackers = room
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.clone())
        .unwrap_or_default();
    *trackers
        .claimed_discard_counts_by_seat
        .entry(discarder_seat)
        .or_default() += 1;
    let _ = update_skill_round_state_in_room_state(room, |round| {
        round.skill_trackers = trackers.clone();
        Ok(())
    });
}

fn apply_events_to_room_in_room_state(
    room: &mut RoomState,
    events: &[GameEvent],
) -> Result<Vec<Value>, String> {
    let mut emitted_messages = Vec::new();
    for event in events {
        match event {
            GameEvent::SkillActivated { seat, skill_id } => {
                emitted_messages.push(round_event_message(
                    "skill_activated",
                    json!({
                        "type": "skill_activated",
                        "seat": seat,
                        "skill_id": skill_id,
                    }),
                ));
            }
            GameEvent::EffectApplied { effect } => {
                update_round_effect_state_in_room_state(room, |effect_state| {
                    effect_state.ongoing.push(effect.clone());
                    Ok(())
                })?;
            }
            GameEvent::EffectExpired { effect_id } => {
                update_round_effect_state_in_room_state(room, |effect_state| {
                    effect_state
                        .ongoing
                        .retain(|effect| effect.effect_id != effect_id.as_str());
                    Ok(())
                })?;
            }
            GameEvent::ViewKnowledgeGranted { knowledge, .. } => {
                update_round_effect_state_in_room_state(room, |effect_state| {
                    effect_state.hidden_knowledge.push(knowledge.clone());
                    Ok(())
                })?;
            }
            GameEvent::RuleOverrideApplied { override_rule } => {
                update_round_effect_state_in_room_state(room, |effect_state| {
                    effect_state.rule_overrides.push(override_rule.clone());
                    Ok(())
                })?;
            }
            GameEvent::SkillTileReplaced {
                seat,
                removed_tile_id,
                replacement_tile,
            } => apply_replace_tile_event_in_room_state(
                room,
                *seat,
                removed_tile_id,
                replacement_tile,
                &mut emitted_messages,
            )?,
            GameEvent::SkillReclaimMeld {
                seat,
                meld_index,
                tile_keys,
            } => apply_reclaim_meld_event_in_room_state(
                room,
                *seat,
                *meld_index,
                tile_keys,
                &mut emitted_messages,
            )?,
            GameEvent::SkillForceDraw {
                seat,
                penalty,
                next_round_penalty,
            } => apply_force_draw_event_in_room_state(
                room,
                *seat,
                *penalty,
                *next_round_penalty,
                &mut emitted_messages,
            )?,
            GameEvent::SkillScoreAdjusted {
                seat,
                delta,
                reason,
            } => apply_score_adjust_event_in_room_state(
                room,
                *seat,
                *delta,
                reason.as_deref(),
                &mut emitted_messages,
            )?,
            _ => {}
        }
    }
    increment_round_version_in_room_state(room)?;
    Ok(emitted_messages)
}

pub fn decline_hu_events(room_state: &RoomState, actor: Seat) -> Result<Vec<GameEvent>, String> {
    decline_hu_events_with_registry(room_state, actor, default_registry())
}

pub fn decline_hu_events_with_registry(
    room_state: &RoomState,
    actor: Seat,
    registry: &dyn SkillRegistry,
) -> Result<Vec<GameEvent>, String> {
    let mut events = Vec::new();
    let Some(loadout) = seat_skill_loadout(room_state, actor) else {
        return Ok(events);
    };
    for skill_instance in &loadout.equipped {
        let Some(definition) = registry.get(&skill_instance.skill_id) else {
            continue;
        };
        let ctx = RuleContext::new(room_state, actor, skill_instance);
        events.extend(definition.on_decline_hu(&ctx)?);
    }
    Ok(events)
}

fn round_event_message(event_type: &str, event: Value) -> Value {
    json!({
        "type": "round_event",
        "payload": {
            "event_type": event_type,
            "event": event,
        }
    })
}

fn pending_multi_hu_candidates(round: &crate::core::state::RoundState) -> Vec<Seat> {
    match round.pending_action.as_ref() {
        Some(PendingAction::ClaimWindow(claim)) => claim
            .claim_window
            .iter()
            .enumerate()
            .filter(|(_, claims)| claims.iter().any(|claim| claim == "hu"))
            .map(|(seat, _)| seat)
            .collect(),
        Some(PendingAction::RobKongWindow(rob)) => rob.offered_hu_seats.clone(),
        _ => Vec::new(),
    }
}

fn compute_tenpai_trackers_in_room_state(
    room: &RoomState,
    seat_count: usize,
) -> (Vec<Seat>, std::collections::BTreeMap<Seat, Vec<String>>) {
    let cache = RoomScoringCache::from_state(room);
    let mut tenpai_seats = Vec::new();
    let mut waits_by_seat = std::collections::BTreeMap::new();
    for seat in 0..seat_count {
        let waits = standard_wait_tile_keys_in_room_state(room, &cache, seat);
        if !waits.is_empty() {
            tenpai_seats.push(seat);
        }
        waits_by_seat.insert(seat, waits);
    }
    (tenpai_seats, waits_by_seat)
}

fn standard_wait_tile_keys_in_room_state(
    room: &RoomState,
    cache: &RoomScoringCache,
    seat: Seat,
) -> Vec<String> {
    const TILE_KEYS: [&str; 34] = [
        "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6",
        "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south",
        "west", "north", "red", "green", "white",
    ];
    TILE_KEYS
        .iter()
        .filter(|tile_key| {
            can_declare_hu_with_cache_for_state(room, cache, seat, Some(tile_key), None)
        })
        .map(|tile_key| (*tile_key).to_string())
        .collect()
}

fn suit_prefix(tile_key: &str) -> Option<&'static str> {
    match tile_key.as_bytes().first().copied() {
        Some(b'w') => Some("w"),
        Some(b't') => Some("t"),
        Some(b'b') => Some("b"),
        _ => None,
    }
}

fn is_suit_five(tile_key: &str) -> bool {
    matches!(tile_key.as_bytes(), [b'w' | b't' | b'b', b'5'])
}

fn is_honor_tile_key(tile_key: &str) -> bool {
    suit_prefix(tile_key).is_none()
}

fn decrement_skill_charge_in_room_state(
    room: &mut RoomState,
    actor: Seat,
    skill_id: &str,
) -> Result<(), String> {
    update_skill_round_state_in_room_state(room, |round| {
        let skill = round
            .players
            .get_mut(actor)
            .and_then(|player| {
                player
                    .skill_loadout
                    .equipped
                    .iter_mut()
                    .find(|skill| skill.skill_id == skill_id)
            })
            .ok_or_else(|| "skill_not_equipped".to_string())?;
        if skill.charges == 0 {
            return Err("skill_no_charges".to_string());
        }
        skill.charges -= 1;
        Ok(())
    })
}

fn increment_round_version_in_room_state(room: &mut RoomState) -> Result<(), String> {
    update_skill_round_state_in_room_state(room, |round| {
        round.version += 1;
        Ok(())
    })
}

fn apply_reclaim_meld_event_in_room_state(
    room: &mut RoomState,
    seat: Seat,
    meld_index: usize,
    tile_keys: &[String],
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let player = round
        .players
        .get(seat)
        .ok_or_else(|| "invalid_action".to_string())?;
    if meld_index >= player.melds.len() {
        return Err("invalid_action".to_string());
    }
    let reclaimed_tiles = tile_keys
        .iter()
        .enumerate()
        .map(|(offset, tile_key)| Tile {
            tile_id: format!("{tile_key}#reclaim:{seat}:{}:{offset}", round.version),
            tile_key: tile_key.to_string(),
            kind: "unknown".to_string(),
            suit: None,
            rank: None,
            name: None,
        })
        .collect::<Vec<_>>();
    update_skill_round_state_in_room_state(room, |round| {
        let player = round
            .players
            .get_mut(seat)
            .ok_or_else(|| "invalid_action".to_string())?;
        if meld_index >= player.melds.len() {
            return Err("invalid_action".to_string());
        }
        player.melds.remove(meld_index);
        player.concealed_tiles.extend(reclaimed_tiles.clone());
        Ok(())
    })?;
    emitted_messages.push(round_event_message(
        "skill_reclaim_meld",
        json!({
            "type": "skill_reclaim_meld",
            "seat": seat,
            "meld_index": meld_index,
            "tile_keys": tile_keys,
        }),
    ));
    Ok(())
}

fn apply_replace_tile_event_in_room_state(
    room: &mut RoomState,
    seat: Seat,
    removed_tile_id: &str,
    replacement_tile: &Tile,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let _ = round
        .players
        .get(seat)
        .and_then(|player| {
            player
                .concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == removed_tile_id)
        })
        .ok_or_else(|| "invalid_action".to_string())?;

    let was_last_live_tile = round.wall.head_index >= round.wall.tail_index;
    let pending_timeout = room
        .pending_timeout
        .as_ref()
        .filter(|timeout| timeout.kind == "active_turn" && timeout.seat_index == seat)
        .map(|timeout| PendingTimeout {
            drawn_tile_id: Some(replacement_tile.tile_id.clone()),
            ..timeout.clone()
        });
    update_skill_round_state_in_room_state(room, |round| {
        let player = round
            .players
            .get_mut(seat)
            .ok_or_else(|| "invalid_action".to_string())?;
        let tile_index = player
            .concealed_tiles
            .iter()
            .position(|tile| tile.tile_id == removed_tile_id)
            .ok_or_else(|| "invalid_action".to_string())?;
        player.concealed_tiles[tile_index] = replacement_tile.clone();
        round.wall.head_index += 1;
        round.last_action_context = LastActionContext {
            kind: "draw".to_string(),
            seat,
            tile_id: Some(replacement_tile.tile_id.clone()),
            from_kong_replacement: false,
            was_last_live_tile,
            was_last_discard: false,
        };
        Ok(())
    })?;
    if let Some(timeout) = pending_timeout {
        room.pending_timeout = Some(timeout);
    }
    note_tracker_draw_in_room_state(room, seat, &replacement_tile.tile_key);

    emitted_messages.push(round_event_message(
        "skill_tile_replaced",
        json!({
            "type": "skill_tile_replaced",
            "seat": seat,
            "removed_tile_id": removed_tile_id,
            "replacement_tile_id": replacement_tile.tile_id,
            "replacement_tile_key": replacement_tile.tile_key,
        }),
    ));
    Ok(())
}

fn apply_force_draw_event_in_room_state(
    room: &mut RoomState,
    seat: Seat,
    penalty: i64,
    next_round_penalty: i64,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    let mut messages =
        crate::rules::standard::settlement::settle_exhaustive_draw_output_in_room_state(room)
            .emitted_messages;
    if let Some(settlement) = room
        .round_state
        .as_ref()
        .and_then(|round| round.settlement.clone())
    {
        let mut settlement = settlement;
        settlement.draw_type = Some("skill_forced".to_string());
        *settlement
            .score_delta
            .total_delta_by_seat
            .entry(seat)
            .or_default() -= penalty;
        *settlement
            .score_delta
            .fan_delta_by_seat
            .entry(seat)
            .or_default() -= penalty;
        update_skill_round_state_in_room_state(room, |round| {
            round.settlement = Some(settlement.clone());
            Ok(())
        })?;
    }
    adjust_match_cumulative_score_in_room_state(room, seat, -penalty);
    if next_round_penalty > 0 {
        set_pending_next_round_win_penalty_in_room_state(room, seat, next_round_penalty);
    }
    messages.push(round_event_message(
        "skill_force_draw",
        json!({
            "type": "skill_force_draw",
            "seat": seat,
            "penalty": penalty,
            "next_round_penalty": next_round_penalty,
        }),
    ));
    emitted_messages.extend(messages);
    Ok(())
}

fn apply_score_adjust_event_in_room_state(
    room: &mut RoomState,
    seat: Seat,
    delta: i64,
    reason: Option<&str>,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    adjust_match_cumulative_score_in_room_state(room, seat, delta);
    emitted_messages.push(round_event_message(
        "skill_score_adjusted",
        json!({
            "type": "skill_score_adjusted",
            "seat": seat,
            "delta": delta,
            "reason": reason.map(ToString::to_string),
        }),
    ));
    Ok(())
}

fn adjust_match_cumulative_score_in_room_state(room: &mut RoomState, seat: Seat, delta: i64) {
    let Some(match_state) = room.match_state.as_ref() else {
        return;
    };
    let mut scores = match_state.cumulative_scores.clone();
    *scores.entry(seat).or_default() += delta;
    let _ = update_skill_match_state_in_room_state(room, |match_state| {
        match_state.cumulative_scores = scores.clone();
        match_state.sync_statistics_to_cumulative_scores();
        Ok(())
    });
}

fn set_pending_next_round_win_penalty_in_room_state(
    room: &mut RoomState,
    seat: Seat,
    penalty: i64,
) {
    let mut trackers = room
        .match_state
        .as_ref()
        .map(|match_state| match_state.skill_trackers.clone())
        .unwrap_or_default();
    trackers
        .zou_wei_shang_ji
        .pending_win_penalty
        .insert(seat, penalty);
    let _ = update_skill_match_state_in_room_state(room, |match_state| {
        match_state.skill_trackers = trackers.clone();
        Ok(())
    });
}

fn update_round_effect_state_in_room_state<F>(
    room: &mut RoomState,
    mut mutate: F,
) -> Result<(), String>
where
    F: FnMut(&mut crate::core::state::EffectState) -> Result<(), String>,
{
    update_skill_round_state_in_room_state(room, |round| mutate(&mut round.effect_state))
}

fn for_each_equipped_skill<F>(
    room_state: &RoomState,
    registry: &dyn SkillRegistry,
    mut callback: F,
) -> Result<(), String>
where
    F: FnMut(&RuleContext<'_>, &dyn SkillDefinition) -> Result<(), String>,
{
    if !skills_enabled(room_state) {
        return Ok(());
    }

    let Some(round) = room_state.round_state.as_ref() else {
        return Ok(());
    };
    for player in &round.players {
        for skill_instance in &player.skill_loadout.equipped {
            let Some(definition) = registry.get(&skill_instance.skill_id) else {
                continue;
            };
            let ctx = RuleContext::new(room_state, player.seat, skill_instance);
            callback(&ctx, definition)?;
        }
    }
    Ok(())
}

fn dedup_projection(projection: &mut SkillProjection) {
    projection.visible_effects.sort_by(|left, right| {
        left.effect_id
            .cmp(&right.effect_id)
            .then(left.effect_type.cmp(&right.effect_type))
    });
    projection
        .visible_effects
        .dedup_by(|left, right| left.effect_id == right.effect_id);

    projection.private_knowledge.sort_by(|left, right| {
        left.viewer
            .cmp(&right.viewer)
            .then(left.target_seat.cmp(&right.target_seat))
            .then(left.source_skill.cmp(&right.source_skill))
            .then(left.tile_ids.cmp(&right.tile_ids))
    });
    projection.private_knowledge.dedup();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::core::event::GameEvent;
    use crate::core::state::{
        EffectState, PlayerRoundState, RoomState, RoundScoreTrackers, RoundState, RuleRuntimeState,
        SkillInstance, SkillLoadout, WallState,
    };

    use super::*;

    fn room_with_skills(skill_ids: &[&str]) -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "skill".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: Vec::new(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                wall: WallState {
                    tiles: Vec::new(),
                    head_index: 0,
                    tail_index: 0,
                },
                players: vec![PlayerRoundState {
                    seat: 0,
                    concealed_tiles: Vec::new(),
                    melds: Vec::new(),
                    flowers: Vec::new(),
                    discards: Vec::new(),
                    skill_loadout: SkillLoadout {
                        equipped: skill_ids
                            .iter()
                            .map(|skill_id| SkillInstance {
                                skill_id: (*skill_id).to_string(),
                                owner: 0,
                                level: 1,
                                rarity: "common".to_string(),
                                remaining_rounds: 2,
                                cooldown: 0,
                                charges: 1,
                                charges_per_round: 1,
                                config: json!({}),
                            })
                            .collect(),
                    },
                }],
                last_discard: None,
                pending_action: None,
                settlement: None,
                version: 1,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: Default::default(),
                rule_state: RuleRuntimeState {
                    enforce_minimum_eight_fan: true,
                },
                effect_state: EffectState::default(),
                restricted_discard_tile_key: None,
                skill_draft: None,
                skill_trackers: Default::default(),
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }

    fn room_with_skill(skill_id: &str) -> RoomState {
        room_with_skills(&[skill_id])
    }

    struct TestSkill;

    impl RuleHook for TestSkill {}

    impl SkillDefinition for TestSkill {
        fn id(&self) -> &str {
            "test-skill"
        }

        fn name(&self) -> &'static str {
            "Test Skill"
        }

        fn activate(
            &self,
            ctx: &mut SkillContext<'_>,
            _target: Option<Seat>,
            _tile_ids: &[TileId],
        ) -> Result<Vec<GameEvent>, String> {
            Ok(vec![GameEvent::SkillActivated {
                seat: ctx.actor,
                skill_id: ctx.skill_instance.skill_id.clone(),
            }])
        }
    }

    struct ActiveOptionSkill;

    impl RuleHook for ActiveOptionSkill {
        fn activation(&self) -> SkillActivation {
            SkillActivation::ActiveTurn
        }
    }

    impl SkillDefinition for ActiveOptionSkill {
        fn id(&self) -> &str {
            "active-option"
        }

        fn name(&self) -> &'static str {
            "Active Option"
        }
    }

    struct ViewHookSkill;

    impl RuleHook for ViewHookSkill {
        fn build_view(
            &self,
            ctx: &RuleContext<'_>,
            local_seat: Seat,
            projection: &mut SkillProjection,
        ) -> Result<(), String> {
            if ctx.actor != local_seat {
                return Ok(());
            }
            projection.visible_effects.push(EffectInstance {
                effect_id: "effect-1".to_string(),
                effect_type: "test-effect".to_string(),
                owner: local_seat,
                target_seats: vec![local_seat],
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                remaining_turns: Some(1),
                stacks: 1,
                consumed: false,
                payload: json!({ "flag": true }),
            });
            projection.private_knowledge.push(KnowledgeEffect {
                viewer: local_seat,
                target_seat: Some(1),
                tile_ids: vec!["w1#0".to_string()],
                tile_keys: vec!["w1".to_string()],
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                description: Some("peek".to_string()),
            });
            Ok(())
        }
    }

    impl SkillDefinition for ViewHookSkill {
        fn id(&self) -> &str {
            "view-hook"
        }

        fn name(&self) -> &'static str {
            "View Hook"
        }
    }

    #[test]
    fn activate_skill_requires_equipped_skill() {
        let room = room_with_skill("other-skill");
        let error = activate_skill(&room, 0, "missing-skill", None, &[])
            .expect_err("missing equipped skill should be rejected");
        assert_eq!(error, "skill_not_equipped");
    }

    #[test]
    fn activate_skill_uses_registered_definition() {
        let room = room_with_skill("test-skill");
        let registry = StaticSkillRegistry::new().with_definition(Arc::new(TestSkill));

        let events =
            activate_skill_with_registry(&room, 0, "test-skill", None, &[], &registry).unwrap();

        assert!(matches!(
            events.as_slice(),
            [GameEvent::SkillActivated {
                seat: 0,
                skill_id
            }] if skill_id == "test-skill"
        ));
    }

    #[test]
    fn action_options_only_include_registered_active_skills() {
        let room = room_with_skills(&["active-option", "passive-skill"]);
        let registry = StaticSkillRegistry::new()
            .with_definition(Arc::new(ActiveOptionSkill))
            .with_definition(Arc::new(TestSkill));

        let options = skill_action_options_with_registry(&room, 0, &registry);

        assert_eq!(options, vec!["skill:active-option".to_string()]);
    }

    #[test]
    fn build_skill_projection_uses_registered_view_hooks() {
        let room = room_with_skill("view-hook");
        let registry = StaticSkillRegistry::new().with_definition(Arc::new(ViewHookSkill));

        let projection = build_skill_projection_with_registry(&room, 0, &registry);

        assert_eq!(projection.visible_effects.len(), 1);
        assert_eq!(projection.visible_effects[0].effect_type, "test-effect");
        assert_eq!(projection.private_knowledge.len(), 1);
        assert_eq!(
            projection.private_knowledge[0].tile_keys,
            vec!["w1".to_string()]
        );
    }
}
