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
        i64::MIN..=-600 => "感动中国大善人",
        -599..=0 => "赛博 ATM",
        1..=400 => "大漏勺",
        401..=600 => "正分守门员",
        601..=800 => "概率论博导",
        801..=1_200 => "大罗金仙",
        1_201..=1_800 => "只手遮天大魔王",
        _ => "太上无极宇宙雀神",
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
        assert_eq!(title_for_points(-600), "感动中国大善人");
        assert_eq!(title_for_points(-599), "赛博 ATM");
        assert_eq!(title_for_points(0), "赛博 ATM");
        assert_eq!(title_for_points(1), "大漏勺");
        assert_eq!(title_for_points(400), "大漏勺");
        assert_eq!(title_for_points(401), "正分守门员");
        assert_eq!(title_for_points(600), "正分守门员");
        assert_eq!(title_for_points(601), "概率论博导");
        assert_eq!(title_for_points(800), "概率论博导");
        assert_eq!(title_for_points(801), "大罗金仙");
        assert_eq!(title_for_points(1_200), "大罗金仙");
        assert_eq!(title_for_points(1_201), "只手遮天大魔王");
        assert_eq!(title_for_points(1_800), "只手遮天大魔王");
        assert_eq!(title_for_points(1_801), "太上无极宇宙雀神");
    }

    #[test]
    fn user_title_display_label_appends_title_with_parentheses() {
        assert_eq!(display_label("Alice", 1_801), "Alice（太上无极宇宙雀神）");
    }
}
