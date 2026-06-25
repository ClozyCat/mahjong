use super::parser::parse_match_text;
use super::test_support::FIXTURE;
use crate::bot_trainer::botzone::{BotZoneAction, BotZoneResult};

#[test]
fn parses_datasets2_round_text_into_botzone_match() {
    let record = parse_match_text(FIXTURE, "fixture.txt").expect("match parses");

    assert_eq!(record.match_id, "344397");
    assert_eq!(record.round_wind, "east");
    assert_eq!(record.dealer_seat, 0);
    assert_eq!(record.deals[0][0], "w8");
    assert_eq!(record.deals[0][3], "north");
    assert_eq!(record.deals[0].len(), 14);
    assert_eq!(record.cumulative_scores, [0, 0, 0, 0]);
    assert_eq!(
        record.result,
        BotZoneResult::Huang {
            score_delta: [0; 4]
        }
    );
    assert_eq!(
        record.events[0].action,
        BotZoneAction::Play {
            tile_key: "white".to_string()
        }
    );
    assert_eq!(
        record.events[1].action,
        BotZoneAction::Peng {
            tile_key: "white".to_string()
        }
    );
    assert_eq!(
        record.events[5].action,
        BotZoneAction::Draw {
            tile_key: "b4".to_string()
        }
    );
    assert_eq!(
        record.events[7].action,
        BotZoneAction::Chi {
            middle_tile_key: "b3".to_string()
        }
    );
}

#[test]
fn skips_flower_only_replacement_draws() {
    let raw = "./2017/2017-12-16/flower-loop.xml
东\t8\t['自摸-1','花牌-2']\t\t自摸
0\t['W1','W2','W3','W4','W5','W6','W7','W8','W9','T1','T2','T3','H1','H2']\t2
1\t['B1','B2','B3','B4','B5','B6','B7','B8','B9','T4','T5','T6','T7']\t0
2\t['J1','J2','J3','F1','F2','F3','F4','W1','W2','W3','B1','B2','B3']\t0
3\t['T1','T2','T3','T4','T5','T6','T7','T8','T9','W7','W8','W9','B7']\t0
0\t摸牌\t['H1']\t
0\t补花\t['H1']\t
0\t补花后摸牌\t['H7']\t
0\t补花\t['H7']\t
0\t补花后摸牌\t['B2']\t
0\t打牌\t['B2']\t
";

    let record = parse_match_text(raw, "flower-loop.txt").expect("match parses");

    assert_eq!(
        record.events[0].action,
        BotZoneAction::Draw {
            tile_key: "b2".to_string()
        }
    );
    assert_eq!(record.events.len(), 2);
}
