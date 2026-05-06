use anyhow::Result;
use chrono::{Duration, SecondsFormat, Utc};

use super::AppContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InviteAvailability {
    Available,
    TargetAlreadyInTable,
    TargetPlayerBusy,
}

pub(crate) fn invite_expires_at() -> String {
    (Utc::now() + Duration::minutes(15)).to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub(crate) async fn invite_availability(
    state: &AppContext,
    invitee_user_id: i64,
    target_table_code: &str,
) -> Result<InviteAvailability> {
    let active_participants = state
        .inner
        .db
        .list_active_table_participants_for_user(invitee_user_id)
        .await?;

    for participant in active_participants {
        if participant.table_code == target_table_code {
            return Ok(InviteAvailability::TargetAlreadyInTable);
        }

        let other_humans = state
            .inner
            .db
            .count_active_other_human_participants(&participant.table_code, invitee_user_id)
            .await?;
        if other_humans > 0 {
            return Ok(InviteAvailability::TargetPlayerBusy);
        }
    }

    Ok(InviteAvailability::Available)
}
