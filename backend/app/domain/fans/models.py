from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class FanRule:
    fan_key: str
    fan_value: int
    category: str
    matcher: Callable[[dict], int]
    value_resolver: Callable[[dict, int, int], list[int]] | None = None
    excludes: tuple[str, ...] = ()
    forbidden_with: tuple[str, ...] = ()
