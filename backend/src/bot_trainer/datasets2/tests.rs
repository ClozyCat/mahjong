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
