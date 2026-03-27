from __future__ import annotations

from app.domain.fans.models import FanRule
from app.domain.fans.rules.distribution_patterns import get_distribution_pattern_rules
from app.domain.fans.rules.honor_patterns import get_honor_pattern_rules
from app.domain.fans.rules.kong_patterns import get_kong_pattern_rules
from app.domain.fans.rules.special_hands import get_special_hand_rules
from app.domain.fans.rules.state_patterns import get_state_pattern_rules
from app.domain.fans.rules.suit_and_sequence_patterns import (
    get_suit_and_sequence_pattern_rules,
)
from app.domain.fans.rules.suit_patterns import get_suit_pattern_rules
from app.domain.fans.rules.timing_patterns import get_timing_pattern_rules
from app.domain.fans.rules.triplet_patterns import get_triplet_pattern_rules
from app.domain.fans.rules.winning_form_patterns import (
    get_winning_form_pattern_rules,
)


def get_registered_fan_rules() -> tuple[FanRule, ...]:
    rules: list[FanRule] = []
    rules.extend(get_timing_pattern_rules())
    rules.extend(get_suit_pattern_rules())
    rules.extend(get_special_hand_rules())
    rules.extend(get_honor_pattern_rules())
    rules.extend(get_triplet_pattern_rules())
    rules.extend(get_distribution_pattern_rules())
    rules.extend(get_suit_and_sequence_pattern_rules())
    rules.extend(get_kong_pattern_rules())
    rules.extend(get_winning_form_pattern_rules())
    rules.extend(get_state_pattern_rules())
    return tuple(rules)
