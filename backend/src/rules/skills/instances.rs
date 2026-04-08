pub use crate::core::state::effect::{SkillInstance, SkillLoadout};

use crate::core::ids::{Seat, SkillId};
use crate::core::state::{PlayerRoundState, RoomState};

pub fn seat_skill_state(room_state: &RoomState, seat: Seat) -> Option<&PlayerRoundState> {
    room_state
        .round_state
        .as_ref()?
        .players
        .iter()
        .find(|player| player.seat == seat)
}

pub fn seat_skill_loadout(room_state: &RoomState, seat: Seat) -> Option<&SkillLoadout> {
    Some(&seat_skill_state(room_state, seat)?.skill_loadout)
}

pub fn find_skill_instance<'a>(
    room_state: &'a RoomState,
    seat: Seat,
    skill_id: &SkillId,
) -> Option<&'a SkillInstance> {
    seat_skill_loadout(room_state, seat)?
        .equipped
        .iter()
        .find(|instance| &instance.skill_id == skill_id)
}
