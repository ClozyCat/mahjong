from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


@dataclass
class CandidateRecord:
    iter: int
    checkpoint: str
    onnx: str
    gate_result: dict[str, Any]
    selected: bool
    promoted: bool


class CandidateBank:
    def __init__(self, candidates: list[CandidateRecord] | None = None) -> None:
        self.candidates = list(candidates or [])

    def add(self, candidate: CandidateRecord) -> None:
        self.candidates = [item for item in self.candidates if item.iter != candidate.iter]
        self.candidates.append(candidate)

    def to_dict(self) -> dict[str, Any]:
        return {"candidates": [asdict(candidate) for candidate in self.candidates]}

    @classmethod
    def from_dict(cls, payload: dict[str, Any]) -> "CandidateBank":
        return cls([
            CandidateRecord(**item)
            for item in payload.get("candidates", [])
        ])


def load_candidate_bank(path: Path) -> CandidateBank:
    if not path.exists():
        return CandidateBank()
    return CandidateBank.from_dict(json.loads(path.read_text(encoding="utf-8")))


def save_candidate_bank(path: Path, bank: CandidateBank) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(bank.to_dict(), indent=2, ensure_ascii=False), encoding="utf-8")


def candidate_score(candidate: CandidateRecord) -> tuple[float, float, float]:
    metrics = candidate.gate_result.get("weighted_metrics", {})
    return (
        float(metrics.get("avg_score_delta", 0.0)),
        float(metrics.get("deal_in_rate", 0.0)),
        float(metrics.get("win_rate", 0.0)),
    )


def select_best_candidate(bank: CandidateBank) -> CandidateRecord:
    selected = [candidate for candidate in bank.candidates if candidate.selected]
    if not selected:
        raise ValueError("candidate bank has no selected candidates")
    return max(selected, key=candidate_score)
