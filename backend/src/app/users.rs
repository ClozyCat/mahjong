use serde::Serialize;

use super::persistence::UserRecord;

struct TitleBand {
    min_points: i64,
    title: &'static str,
    description: &'static str,
}

const TITLE_BANDS: [TitleBand; 15] = [
    TitleBand {
        min_points: -50,
        title: "散财童子",
        description: "分低到像是在做慈善，牌桌上的财神爷，专程给三家送温暖。",
    },
    TitleBand {
        min_points: 50,
        title: "点炮能手",
        description: "别人胡牌你助攻，精准一发点炮，堪称对手的最佳队友。",
    },
    TitleBand {
        min_points: 150,
        title: "慈善赌王",
        description: "输得荡气回肠，用实力诠释“重在参与”，善款捐赠量全桌第一。",
    },
    TitleBand {
        min_points: 250,
        title: "常年陪跑",
        description: "永远在陪打，从未拿过胜果，存在感约等于牌桌背景板。",
    },
    TitleBand {
        min_points: 350,
        title: "失魂雀士",
        description: "摸牌犹犹豫豫，出牌神游天外，魂儿没带来，分被带走了。",
    },
    TitleBand {
        min_points: 450,
        title: "迷途麻客",
        description: "知道牌型但不知道方向，经常在“该攻该守”的十字路口迷路。",
    },
    TitleBand {
        min_points: 550,
        title: "入门雀友",
        description: "终于摸清国标规则，但离赢牌还隔着一整本《麻将高阶走位学》。",
    },
    TitleBand {
        min_points: 650,
        title: "初露锋芒",
        description: "偶尔能胡出像样的牌，开始让对手觉得“这人有点意思”。",
    },
    TitleBand {
        min_points: 750,
        title: "稳扎稳打",
        description: "不求惊天动地，只求少点炮、多蹭番，积分渐渐稳住了。",
    },
    TitleBand {
        min_points: 850,
        title: "牌桌猎手",
        description: "听牌快、胡牌准，一旦嗅到机会就像猎手，迅速拿下战果。",
    },
    TitleBand {
        min_points: 950,
        title: "运筹帷幄",
        description: "舍牌有章法，做牌有远见，仿佛手里拿着一本剧本在雀桌上导戏。",
    },
    TitleBand {
        min_points: 1050,
        title: "不败战将",
        description: "败局极少出现，连续获胜已成常态，属于让人不想匹配的存在。",
    },
    TitleBand {
        min_points: 1150,
        title: "雀坛传说",
        description: "打法已成谈资，排名名震一方，普通雀友在茶馆里会提到你的大名。",
    },
    TitleBand {
        min_points: 1250,
        title: "封神雀圣",
        description: "近乎神化的控局能力，坐上桌就自带压迫光环，只差一个正式加冕。",
    },
    TitleBand {
        min_points: 1350,
        title: "至尊雀神",
        description: "国标之巅，俯瞰众生。你的存在本身就是对“运气”二字的不屑。",
    },
];

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

pub(crate) fn title_for_points(points: i64) -> &'static str {
    title_band_for_points(points).title
}

pub(crate) fn title_description_for_points(points: i64) -> &'static str {
    title_band_for_points(points).description
}

fn title_band_for_points(points: i64) -> &'static TitleBand {
    TITLE_BANDS
        .iter()
        .rev()
        .find(|band| points >= band.min_points)
        .unwrap_or(&TITLE_BANDS[0])
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
        bio: title_description_for_points(user.points).to_string(),
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
        assert_eq!(title_for_points(-1_000), "散财童子");
        assert_eq!(title_for_points(-50), "散财童子");
        assert_eq!(title_for_points(49), "散财童子");
        assert_eq!(title_for_points(50), "点炮能手");
        assert_eq!(title_for_points(149), "点炮能手");
        assert_eq!(title_for_points(150), "慈善赌王");
        assert_eq!(title_for_points(549), "迷途麻客");
        assert_eq!(title_for_points(550), "入门雀友");
        assert_eq!(title_for_points(649), "入门雀友");
        assert_eq!(title_for_points(650), "初露锋芒");
        assert_eq!(title_for_points(1_349), "封神雀圣");
        assert_eq!(title_for_points(1_350), "至尊雀神");
        assert_eq!(title_for_points(9_999), "至尊雀神");
    }

    #[test]
    fn user_title_display_label_appends_title_with_parentheses() {
        assert_eq!(display_label("Alice", 550), "Alice（入门雀友）");
    }

    #[test]
    fn user_title_description_matches_public_profile_copy() {
        assert_eq!(
            title_description_for_points(650),
            "偶尔能胡出像样的牌，开始让对手觉得“这人有点意思”。"
        );
    }
}
