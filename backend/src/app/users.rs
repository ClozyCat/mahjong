use serde::Serialize;

use super::persistence::UserRecord;

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
}

pub(crate) fn title_for_points(points: i64) -> &'static str {
    match points {
        i64::MIN..=-1 => "乞丐",
        0..=499 => "平民",
        500..=1_999 => "小康",
        2_000..=4_999 => "富豪",
        _ => "财神",
    }
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
        title: title_for_points(user.points).to_string(),
        display_label: display_label(&user.display_name, user.points),
        bio: user.bio.clone(),
        avatar: user.avatar.clone(),
        active_table_code: None,
    }
}

pub(crate) fn public_user_view_with_active_table(
    user: &UserRecord,
    active_table_code: Option<String>,
) -> PublicUserView {
    PublicUserView {
        active_table_code,
        ..public_user_view(user)
    }
}

#[cfg(test)]
mod tests {
    use super::{display_label, title_for_points};

    #[test]
    fn user_title_thresholds_follow_design_ranges() {
        assert_eq!(title_for_points(-1), "乞丐");
        assert_eq!(title_for_points(0), "平民");
        assert_eq!(title_for_points(499), "平民");
        assert_eq!(title_for_points(500), "小康");
        assert_eq!(title_for_points(1_999), "小康");
        assert_eq!(title_for_points(2_000), "富豪");
        assert_eq!(title_for_points(4_999), "富豪");
        assert_eq!(title_for_points(5_000), "财神");
    }

    #[test]
    fn user_title_display_label_appends_title_with_parentheses() {
        assert_eq!(display_label("Alice", 2_000), "Alice（富豪）");
    }
}
