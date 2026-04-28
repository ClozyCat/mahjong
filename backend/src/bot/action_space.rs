pub(crate) const TILE_KIND_COUNT: usize = 34;
pub(crate) const CLAIM_ACTION_COUNT: usize = 7;
pub(crate) const SELF_KONG_ACTION_COUNT: usize = 3;

pub(crate) const TILE_KEYS: [&str; TILE_KIND_COUNT] = [
    "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6",
    "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south",
    "west", "north", "red", "green", "white",
];

pub(crate) const CLAIM_ACTIONS: [&str; CLAIM_ACTION_COUNT] = [
    "pass",
    "hu",
    "pung",
    "kong",
    "chow_left",
    "chow_mid",
    "chow_right",
];

pub(crate) const SELF_KONG_ACTIONS: [&str; SELF_KONG_ACTION_COUNT] =
    ["pass", "concealed_kong", "add_kong"];

pub(crate) fn tile_index(tile_key: &str) -> Option<usize> {
    TILE_KEYS.iter().position(|key| *key == tile_key)
}

pub(crate) fn tile_key_for_index(index: usize) -> Option<&'static str> {
    TILE_KEYS.get(index).copied()
}

pub(crate) fn claim_action_index(action: &str) -> Option<usize> {
    CLAIM_ACTIONS.iter().position(|candidate| *candidate == action)
}

pub(crate) fn self_kong_action_index(action: &str) -> Option<usize> {
    SELF_KONG_ACTIONS
        .iter()
        .position(|candidate| *candidate == action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_v2_tile_keys_match_backend_order() {
        assert_eq!(TILE_KEYS[0], "w1");
        assert_eq!(TILE_KEYS[8], "w9");
        assert_eq!(TILE_KEYS[9], "t1");
        assert_eq!(TILE_KEYS[18], "b1");
        assert_eq!(TILE_KEYS[27], "east");
        assert_eq!(TILE_KEYS[33], "white");
    }

    #[test]
    fn action_indexes_are_stable() {
        assert_eq!(claim_action_index("pass"), Some(0));
        assert_eq!(claim_action_index("hu"), Some(1));
        assert_eq!(claim_action_index("chow_right"), Some(6));
        assert_eq!(self_kong_action_index("pass"), Some(0));
        assert_eq!(self_kong_action_index("add_kong"), Some(2));
    }
}
