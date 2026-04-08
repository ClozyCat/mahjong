use super::evaluator::{FanRule, registered_fan_rules};

pub(crate) struct StandardFanTable;

impl StandardFanTable {
    pub(crate) fn rules() -> &'static [FanRule] {
        registered_fan_rules()
    }
}
