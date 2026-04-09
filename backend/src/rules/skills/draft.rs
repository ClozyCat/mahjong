use std::collections::BTreeMap;

use chrono::{SecondsFormat, TimeDelta, Utc};
use rand::Rng;
use rand::prelude::IndexedRandom;
use rand::seq::SliceRandom;
use serde_json::json;

use crate::core::ids::{Seat, SkillId};
use crate::core::state::{
    RoomState, SkillDraftChoice, SkillDraftOffer, SkillDraftState, SkillDraftStatus, SkillInstance,
    SkillLoadout, SkillRarity,
};
use crate::rules::standard::runtime::sync_pending_timeout_in_room_state;

use super::catalog::{
    SkillKind, active_uses_per_round_for_skill, catalog, entry, rarity_weights, stratagem_skill_ids,
};

pub fn clear_skill_loadouts_for_new_match_in_room_state(room: &mut RoomState) {
    for seat in &mut room.seats {
        seat.skill_loadout = SkillLoadout::default();
    }
    if let Some(round) = room.round_state.as_mut() {
        for player in &mut round.players {
            player.skill_loadout = SkillLoadout::default();
        }
        round.skill_draft = None;
    }
}

pub fn roll_skill_loadouts_forward_in_room_state(room: &mut RoomState) {
    advance_skill_loadouts_for_next_round_in_room_state(room);
}

pub fn initialize_skill_draft_for_round_in_room_state(room: &mut RoomState) {
    let _ = begin_round_skill_draft_in_room_state(room);
    sync_pending_timeout_in_room_state(room);
}

pub fn advance_skill_loadouts_for_next_round_in_room_state(room: &mut RoomState) {
    for seat in &mut room.seats {
        advance_loadout(&mut seat.skill_loadout);
    }
}

pub fn begin_round_skill_draft_in_room_state(room: &mut RoomState) -> Result<(), String> {
    let should_offer = should_offer_skills(room);
    let cycle_key = current_cycle_key(room)?;
    let cycle_label = current_cycle_label(room)?;
    let Some(round) = room.round_state.as_mut() else {
        return Ok(());
    };

    round.skill_draft = None;
    if !should_offer {
        return Ok(());
    }

    let deadline_at = (Utc::now() + TimeDelta::seconds(catalog().selection.duration_seconds))
        .to_rfc3339_opts(SecondsFormat::Micros, true);

    let mut rng = rand::rng();
    let mut offers_by_seat = BTreeMap::new();
    for player in &round.players {
        offers_by_seat.insert(
            player.seat,
            SkillDraftOffer {
                seat: player.seat,
                status: SkillDraftStatus::Pending,
                options: draw_skill_offer_choices(&mut rng)?,
                selected_skill_id: None,
                selected_rarity: None,
            },
        );
    }

    round.skill_draft = Some(SkillDraftState {
        cycle_key: cycle_key.clone(),
        cycle_label,
        round_id: round.round_id.clone(),
        deadline_at,
        offers_by_seat,
    });

    let bot_seats = room
        .seats
        .iter()
        .filter(|seat| seat.is_bot)
        .map(|seat| seat.seat_index)
        .collect::<Vec<_>>();
    for seat in bot_seats {
        auto_select_offer_for_bot(room, seat, &cycle_key)?;
    }

    if !room
        .round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .is_some_and(SkillDraftState::is_active)
    {
        room.round_state
            .as_mut()
            .and_then(|round| round.skill_draft.take());
    }

    Ok(())
}

pub fn select_skill_offer_in_room_state(
    room: &mut RoomState,
    seat: Seat,
    skill_id: &str,
) -> Result<(), String> {
    let (choice, cycle_key) = pending_offer_choice(room, seat, skill_id)?;
    let skill_instance = build_skill_instance(seat, &choice, &cycle_key)?;
    apply_skill_selection(
        room,
        seat,
        Some(skill_instance),
        Some(choice.skill_id),
        Some(choice.rarity),
    );
    Ok(())
}

pub fn decline_skill_offer_in_room_state(room: &mut RoomState, seat: Seat) -> Result<(), String> {
    ensure_pending_offer(room, seat)?;
    apply_skill_selection(room, seat, None, None, None);
    Ok(())
}

pub fn resolve_due_skill_draft_in_room_state(
    room: &mut RoomState,
) -> Result<Vec<serde_json::Value>, String> {
    let pending_seats = room
        .round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .map(|draft| {
            draft
                .offers_by_seat
                .iter()
                .filter_map(|(seat, offer)| {
                    (offer.status == SkillDraftStatus::Pending).then_some(*seat)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for seat in pending_seats {
        apply_skill_selection(room, seat, None, None, None);
    }

    Ok(Vec::new())
}

pub fn next_skill_draft_deadline(room: &RoomState) -> Option<&str> {
    let draft = room.round_state.as_ref()?.skill_draft.as_ref()?;
    draft.is_active().then_some(draft.deadline_at.as_str())
}

pub fn all_skill_draft_responses_complete(room: &RoomState) -> bool {
    room.round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .map(|draft| !draft.is_active())
        .unwrap_or(true)
}

pub fn clear_skill_draft_in_room_state(room: &mut RoomState) {
    if let Some(round) = room.round_state.as_mut() {
        round.skill_draft = None;
    }
}

fn pending_offer_choice(
    room: &RoomState,
    seat: Seat,
    skill_id: &str,
) -> Result<(SkillDraftChoice, String), String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let draft = round
        .skill_draft
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let offer = draft
        .offers_by_seat
        .get(&seat)
        .ok_or_else(|| "invalid_action".to_string())?;
    if offer.status != SkillDraftStatus::Pending {
        return Err("invalid_action".to_string());
    }
    let choice = offer
        .options
        .iter()
        .find(|choice| choice.skill_id == skill_id)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    Ok((choice, draft.cycle_key.clone()))
}

fn ensure_pending_offer(room: &RoomState, seat: Seat) -> Result<(), String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let draft = round
        .skill_draft
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let offer = draft
        .offers_by_seat
        .get(&seat)
        .ok_or_else(|| "invalid_action".to_string())?;
    if offer.status != SkillDraftStatus::Pending {
        return Err("invalid_action".to_string());
    }
    Ok(())
}

fn build_skill_instance(
    owner: Seat,
    choice: &SkillDraftChoice,
    cycle_key: &str,
) -> Result<SkillInstance, String> {
    let kind = entry(&choice.skill_id)
        .map(|entry| entry.skill_type)
        .ok_or_else(|| "invalid_action".to_string())?;
    let remaining_rounds = catalog().selection.duration_rounds;
    let remaining_activations = if kind == SkillKind::Active {
        active_uses_per_round_for_skill(&choice.skill_id, choice.rarity.level())
    } else {
        0
    };
    Ok(SkillInstance {
        skill_id: choice.skill_id.clone(),
        owner,
        level: choice.rarity.level(),
        rarity: match choice.rarity {
            SkillRarity::Common => "common".to_string(),
            SkillRarity::Rare => "rare".to_string(),
            SkillRarity::Epic => "epic".to_string(),
        },
        remaining_rounds,
        cooldown: 0,
        charges: remaining_activations,
        charges_per_round: remaining_activations,
        config: json!({
            "cycle_key": cycle_key,
            "rarity": match choice.rarity {
                SkillRarity::Common => "common",
                SkillRarity::Rare => "rare",
                SkillRarity::Epic => "epic",
            },
            "remaining_rounds": remaining_rounds,
        }),
    })
}

fn apply_skill_selection(
    room: &mut RoomState,
    seat: Seat,
    selected_skill: Option<SkillInstance>,
    selected_skill_id: Option<SkillId>,
    selected_rarity: Option<SkillRarity>,
) {
    let next_loadout = SkillLoadout {
        equipped: selected_skill.into_iter().collect(),
    };

    if let Some(seat_state) = room
        .seats
        .iter_mut()
        .find(|seat_state| seat_state.seat_index == seat)
    {
        seat_state.skill_loadout = next_loadout.clone();
    }
    if let Some(player) = room
        .round_state
        .as_mut()
        .and_then(|round| round.players.iter_mut().find(|player| player.seat == seat))
    {
        player.skill_loadout = next_loadout;
    }
    if let Some(offer) = room
        .round_state
        .as_mut()
        .and_then(|round| round.skill_draft.as_mut())
        .and_then(|draft| draft.offers_by_seat.get_mut(&seat))
    {
        offer.status = if selected_skill_id.is_some() {
            SkillDraftStatus::Selected
        } else {
            SkillDraftStatus::Declined
        };
        offer.selected_skill_id = selected_skill_id;
        offer.selected_rarity = selected_rarity;
    }
    finalize_draft_if_complete(room);
}

fn finalize_draft_if_complete(room: &mut RoomState) {
    let is_complete = room
        .round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .is_some_and(|draft| !draft.is_active());
    if is_complete {
        if let Some(round) = room.round_state.as_mut() {
            round.skill_draft = None;
        }
    }
}

fn auto_select_offer_for_bot(
    room: &mut RoomState,
    seat: Seat,
    cycle_key: &str,
) -> Result<(), String> {
    let mut rng = rand::rng();
    let choice = room
        .round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .and_then(|draft| draft.offers_by_seat.get(&seat))
        .and_then(|offer| offer.options.choose(&mut rng).cloned());
    let Some(choice) = choice else {
        return Ok(());
    };

    let skill_instance = build_skill_instance(seat, &choice, cycle_key)?;
    apply_skill_selection(
        room,
        seat,
        Some(skill_instance),
        Some(choice.skill_id),
        Some(choice.rarity),
    );
    Ok(())
}

fn should_offer_skills(room: &RoomState) -> bool {
    room.mode == "skill"
        && room.phase == "playing"
        && room
            .match_state
            .as_ref()
            .is_some_and(|match_state| match_state.hand_number % 2 == 1)
}

fn current_cycle_key(room: &RoomState) -> Result<String, String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let match_state = room
        .match_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    Ok(format!("{}-{}", round.round_wind, match_state.hand_number))
}

fn current_cycle_label(room: &RoomState) -> Result<String, String> {
    let round = room
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let match_state = room
        .match_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let wind = match round.round_wind.as_str() {
        "east" => "东",
        "south" => "南",
        "west" => "西",
        "north" => "北",
        other => other,
    };
    let start = match_state.hand_number;
    let end = (start + 1).min(4);
    Ok(format!("{wind}{start}~{wind}{end}局"))
}

fn draw_skill_offer_choices(rng: &mut impl Rng) -> Result<Vec<SkillDraftChoice>, String> {
    let mut pool = stratagem_skill_ids().to_vec();
    pool.shuffle(rng);
    let picked = pool
        .into_iter()
        .take(catalog().selection.offer_count)
        .collect::<Vec<_>>();
    if picked.len() < catalog().selection.offer_count {
        return Err("invalid_action".to_string());
    }

    Ok(picked
        .into_iter()
        .map(|skill_id| SkillDraftChoice {
            skill_id,
            rarity: roll_rarity(rng),
        })
        .collect())
}

fn roll_rarity(rng: &mut impl Rng) -> SkillRarity {
    let roll = rng.random_range(0..100);
    let mut lower_bound = 0;
    for (rarity, weight) in rarity_weights() {
        let upper_bound = lower_bound + usize::from(*weight);
        if roll < upper_bound {
            return match rarity {
                super::catalog::SkillRarity::Common => SkillRarity::Common,
                super::catalog::SkillRarity::Rare => SkillRarity::Rare,
                super::catalog::SkillRarity::Epic => SkillRarity::Epic,
            };
        }
        lower_bound = upper_bound;
    }
    SkillRarity::Common
}

fn advance_loadout(loadout: &mut SkillLoadout) {
    let mut next_equipped = Vec::new();
    for mut skill in loadout.equipped.drain(..) {
        let remaining_rounds = skill.remaining_rounds.saturating_sub(1);
        if remaining_rounds == 0 {
            continue;
        }

        skill.remaining_rounds = remaining_rounds;
        let remaining_activations = match entry(&skill.skill_id).map(|entry| entry.skill_type) {
            Some(SkillKind::Active) => {
                active_uses_per_round_for_skill(&skill.skill_id, skill.level)
            }
            _ => 0,
        };
        skill.charges = remaining_activations;
        skill.charges_per_round = remaining_activations;
        if let Some(config) = skill.config.as_object_mut() {
            config.insert("remaining_rounds".to_string(), json!(remaining_rounds));
        }
        next_equipped.push(skill);
    }
    loadout.equipped = next_equipped;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::core::state::{
        LastActionContext, MatchState, PlayerRoundState, RoomState, RoundScoreTrackers,
        RoundState, RuleRuntimeState, SkillDraftChoice, SkillInstance, SkillLoadout,
        SkillRarity, WallState,
    };

    use super::{advance_loadout, begin_round_skill_draft_in_room_state, build_skill_instance};

    #[test]
    fn build_skill_instance_uses_tier_activation_limit() {
        let choice = SkillDraftChoice {
            skill_id: "jin_chan_tuo_qiao".to_string(),
            rarity: SkillRarity::Epic,
        };

        let skill =
            build_skill_instance(0, &choice, "east-1").expect("skill instance should build");

        assert_eq!(skill.charges, 2);
        assert_eq!(skill.charges_per_round, 2);
    }

    #[test]
    fn advance_loadout_resets_active_charges_from_catalog() {
        let mut loadout = SkillLoadout {
            equipped: vec![SkillInstance {
                skill_id: "tou_liang_huan_zhu".to_string(),
                owner: 0,
                level: 3,
                rarity: "epic".to_string(),
                remaining_rounds: 2,
                cooldown: 0,
                charges: 0,
                charges_per_round: 0,
                config: json!({ "remaining_rounds": 2 }),
            }],
        };

        advance_loadout(&mut loadout);

        assert_eq!(loadout.equipped.len(), 1);
        assert_eq!(loadout.equipped[0].remaining_rounds, 1);
        assert_eq!(loadout.equipped[0].charges, 2);
        assert_eq!(loadout.equipped[0].charges_per_round, 2);
    }

    #[test]
    fn normal_mode_does_not_offer_round_skills() {
        let mut room = skill_mode_room("normal");

        begin_round_skill_draft_in_room_state(&mut room).expect("draft initialization should succeed");

        assert!(room.round_state.as_ref().and_then(|round| round.skill_draft.as_ref()).is_none());
    }

    #[test]
    fn skill_mode_offers_round_skills_on_odd_hands() {
        let mut room = skill_mode_room("skill");

        begin_round_skill_draft_in_room_state(&mut room).expect("draft initialization should succeed");

        assert!(room.round_state.as_ref().and_then(|round| round.skill_draft.as_ref()).is_some());
    }

    fn skill_mode_room(mode: &str) -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: mode.to_string(),
            test_mode: mode == "test",
            enforce_minimum_eight_fan: true,
            seats: vec![],
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: Default::default(),
                match_finished: false,
                last_completed_round_id: None,
                skill_trackers: Default::default(),
            }),
            round_state: Some(RoundState {
                round_id: "east-1-room42".to_string(),
                dealer_seat: 0,
                current_actor: 0,
                wall: WallState::default(),
                players: vec![
                    PlayerRoundState {
                        seat: 0,
                        concealed_tiles: vec![],
                        melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                        skill_loadout: SkillLoadout::default(),
                    },
                    PlayerRoundState {
                        seat: 1,
                        concealed_tiles: vec![],
                        melds: vec![],
                        flowers: vec![],
                        discards: vec![],
                        skill_loadout: SkillLoadout::default(),
                    },
                ],
                last_discard: None,
                pending_action: None,
                phase: "playing".to_string(),
                settlement: None,
                version: 1,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: LastActionContext::default(),
                rule_state: RuleRuntimeState {
                    enforce_minimum_eight_fan: true,
                },
                effect_state: Default::default(),
                restricted_discard_tile_key: None,
                skill_draft: None,
                skill_trackers: Default::default(),
                round_wind: "east".to_string(),
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }
}
