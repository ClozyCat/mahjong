use serde::Serialize;

use super::persistence::UserRecord;

const POINTS_PER_LEVEL: i64 = 50;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct PublicUserView {
    pub(crate) user_id: i64,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) points: i64,
    pub(crate) title: String,
    pub(crate) display_label: String,
    pub(crate) bio: String,
    pub(crate) avatar: Option<String>,
    pub(crate) active_table_code: Option<String>,
    pub(crate) active_table_phase: Option<String>,
    pub(crate) is_special_bot: bool,
}

pub(crate) fn title_for_points(points: i64) -> String {
    if points <= 0 {
        return "LV0".to_string();
    }

    format!("LV{}", points / POINTS_PER_LEVEL)
}

pub(crate) fn title_description_for_points(points: i64) -> String {
    format!("{} 段位", title_for_points(points))
}

pub(crate) fn display_label(display_name: &str, points: i64) -> String {
    format!("{display_name}（{}）", title_for_points(points))
}

pub(crate) fn public_user_view(user: &UserRecord) -> PublicUserView {
    PublicUserView {
        user_id: user.user_id,
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        points: user.points,
        title: title_for_points(user.points),
        display_label: display_label(&user.display_name, user.points),
        bio: title_description_for_points(user.points),
        avatar: user.avatar.clone(),
        active_table_code: None,
        active_table_phase: None,
        is_special_bot: false,
    }
}

pub(crate) fn public_user_view_with_active_table(
    user: &UserRecord,
    active_table_code: Option<String>,
    active_table_phase: Option<String>,
    is_special_bot: bool,
) -> PublicUserView {
    PublicUserView {
        active_table_code,
        active_table_phase,
        is_special_bot,
        ..public_user_view(user)
    }
}

#[cfg(test)]
mod tests {
    use super::{display_label, title_description_for_points, title_for_points};

    #[test]
    fn user_title_thresholds_are_lower_inclusive_upper_exclusive() {
        assert_eq!(title_for_points(-1_000), "LV0");
        assert_eq!(title_for_points(0), "LV0");
        assert_eq!(title_for_points(1), "LV0");
        assert_eq!(title_for_points(49), "LV0");
        assert_eq!(title_for_points(50), "LV1");
        assert_eq!(title_for_points(99), "LV1");
        assert_eq!(title_for_points(100), "LV2");
        assert_eq!(title_for_points(599), "LV11");
        assert_eq!(title_for_points(600), "LV12");
        assert_eq!(title_for_points(649), "LV12");
        assert_eq!(title_for_points(650), "LV13");
        assert_eq!(title_for_points(700), "LV14");
        assert_eq!(title_for_points(750), "LV15");
        assert_eq!(title_for_points(9_999), "LV199");
    }

    #[test]
    fn user_title_display_label_appends_title_with_parentheses() {
        assert_eq!(display_label("Alice", 600), "Alice（LV12）");
    }

    #[test]
    fn user_title_description_matches_public_profile_copy() {
        assert_eq!(title_description_for_points(650), "LV13 段位");
    }
}
