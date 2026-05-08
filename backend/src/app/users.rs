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
        title: "全自动点炮机",
        description: "牌桌上的活菩萨。只要坐上牌桌，唯一的作用就是把自己的分精准地喂进别人的嘴里。",
    },
    TitleBand {
        min_points: 50,
        title: "首席散财童子",
        description: "主打一个陪伴。输赢已经不重要了，主要是喜欢看着别人赢钱时开心的笑容。",
    },
    TitleBand {
        min_points: 150,
        title: "国标八番困难户",
        description: "对国标“起和八番”的门槛有着深深的恐惧，好不容易听牌了，一算番数只有可怜的六番，终生在及格线边缘挣扎。",
    },
    TitleBand {
        min_points: 250,
        title: "视力测试员",
        description: "打麻将对他们来说只是一项简单的物理运动：把牌摸起来，看清花色，然后打出去。不包含任何脑力计算。",
    },
    TitleBand {
        min_points: 350,
        title: "薛定谔的听牌",
        description: "别人根本猜不透他有没有听牌，因为甚至连他自己都不知道自己听了什么、差几番。",
    },
    TitleBand {
        min_points: 450,
        title: "间歇性好运携带者",
        description: "毫无技术可言，能赢全靠发牌员心情好。一旦运气用光，瞬间跌回“全自动点炮机”。",
    },
    TitleBand {
        min_points: 550,
        title: "熟练的码牌工",
        description: "已经脱离了新手的低级趣味，不仅码牌速度快，甚至偶尔还能看懂别人在做什么牌。",
    },
    TitleBand {
        min_points: 650,
        title: "弹性拆牌艺术家",
        description: "深谙“好死不如赖活着”的麻将哲学。前一秒还在雄心勃勃地规划大牌，下一秒察觉到危险，立刻就能把一手好牌拆得连亲妈都不认识。怂得行云流水，退得理直气壮。",
    },
    TitleBand {
        min_points: 750,
        title: "牌池人体扫描仪",
        description: "双眼犹如X光，死死盯着牌河里的每一张废牌。想从他眼皮子底下骗吃骗碰？不存在的。他不仅知道你想要什么牌，甚至还能反手给你喂一口难受的“毒药”。",
    },
    TitleBand {
        min_points: 850,
        title: "精准控分专家",
        description: "国标麻将的精算师。手里捏着无数种组合方式，总能以最刁钻的角度、凑出最性价比的番数拿下对局。",
    },
    TitleBand {
        min_points: 950,
        title: "牌桌读心者",
        description: "你的一个眼神，一次犹豫，他就已经知道你听的是哪张牌了。在他面前，其他玩家仿佛是透明的。",
    },
    TitleBand {
        min_points: 1050,
        title: "降维打击操盘手",
        description: "别人是在打麻将，他是在做数学建模。通过极强的逻辑推理，将对手玩弄于股掌之间。",
    },
    TitleBand {
        min_points: 1150,
        title: "人形量子算番机",
        description: "摸牌的瞬间，大脑已经计算完了国标麻将所有可能的番种组合和胡牌概率。算力之强，令服务器汗颜。",
    },
    TitleBand {
        min_points: 1250,
        title: "气运与实力之主",
        description: "科学与玄学的完美结合。不仅算无遗策，连老天爷都站在他这边，想摸什么就来什么。",
    },
    TitleBand {
        min_points: 1350,
        title: "言出法随真雀神",
        description: "凡人不可直视的巅峰境界。他坐庄，那是神在审判；他打牌，那是神在赐教。牌桌之上，他就是唯一的真理。",
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
    use super::{display_label, title_description_for_points, title_for_points};

    #[test]
    fn user_title_thresholds_are_lower_inclusive_upper_exclusive() {
        assert_eq!(title_for_points(-1_000), "全自动点炮机");
        assert_eq!(title_for_points(-50), "全自动点炮机");
        assert_eq!(title_for_points(49), "全自动点炮机");
        assert_eq!(title_for_points(50), "首席散财童子");
        assert_eq!(title_for_points(149), "首席散财童子");
        assert_eq!(title_for_points(150), "国标八番困难户");
        assert_eq!(title_for_points(549), "间歇性好运携带者");
        assert_eq!(title_for_points(550), "熟练的码牌工");
        assert_eq!(title_for_points(649), "熟练的码牌工");
        assert_eq!(title_for_points(650), "弹性拆牌艺术家");
        assert_eq!(title_for_points(1_349), "气运与实力之主");
        assert_eq!(title_for_points(1_350), "言出法随真雀神");
        assert_eq!(title_for_points(9_999), "言出法随真雀神");
    }

    #[test]
    fn user_title_display_label_appends_title_with_parentheses() {
        assert_eq!(display_label("Alice", 550), "Alice（熟练的码牌工）");
    }

    #[test]
    fn user_title_description_matches_public_profile_copy() {
        assert_eq!(
            title_description_for_points(650),
            "深谙“好死不如赖活着”的麻将哲学。前一秒还在雄心勃勃地规划大牌，下一秒察觉到危险，立刻就能把一手好牌拆得连亲妈都不认识。怂得行云流水，退得理直气壮。"
        );
    }
}
