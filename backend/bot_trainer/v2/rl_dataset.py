from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Callable

import torch
from torch.utils.data import Dataset

DISCARD_SEQUENCE_LENGTH = 32
DISCARD_EVENT_FEATURE_COUNT = 40


class ArenaTrajectoryDataset(Dataset):
    def __init__(
        self,
        path: Path,
        gamma: float = 0.99,
        gae_lambda: float = 0.95,
        policy_id: str | None = None,
        cache_path: Path | None = None,
    ) -> None:
        if cache_path is not None:
            cache = load_or_build_tensor_cache(
                path,
                cache_path,
                gamma=gamma,
                gae_lambda=gae_lambda,
                policy_id=policy_id,
            )
            self.rows = list(cache["rows"])
            self.advantages = list(cache["advantages"])
            self.returns = list(cache["returns"])
            self.tensors = dict(cache["tensors"])
            return
        rows = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8-sig").splitlines()
            if line.strip()
        ]
        if policy_id is not None:
            rows = [row for row in rows if row.get("policy_id") == policy_id]
        self.rows = rows
        self.advantages, self.returns = compute_gae_for_rows(
            self.rows,
            gamma=gamma,
            gae_lambda=gae_lambda,
        )

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
        row = dict(self.rows[index])
        row["advantage"] = self.advantages[index]
        if hasattr(self, "tensors"):
            item = {
                key: value[index]
                for key, value in self.tensors.items()
            }
            item["return"] = self.tensors["return"][index]
            item["advantage"] = self.tensors["advantage"][index]
            return item
        return encode_row(row, self.returns[index])

    def recompute_values_and_gae(
        self,
        value_fn: Callable[[dict[str, torch.Tensor]], torch.Tensor],
        device: torch.device,
        batch_size: int = 256,
        gamma: float = 0.995,
        gae_lambda: float = 0.95,
    ) -> None:
        from torch.utils.data import DataLoader

        loader = DataLoader(self, batch_size=batch_size, shuffle=False)
        values: list[float] = []
        with torch.no_grad():
            for batch in loader:
                batch = {k: v.to(device) for k, v in batch.items()}
                v = value_fn(batch)
                if isinstance(v, torch.Tensor):
                    while v.dim() > 1:
                        v = v.squeeze(-1)
                    values.extend(v.detach().cpu().tolist())
                else:
                    values.extend([float(v)] * len(batch["reward"]))

        if len(values) != len(self.rows):
            raise ValueError(
                f"Value count mismatch: {len(values)} values for {len(self.rows)} rows"
            )

        for row, val in zip(self.rows, values, strict=True):
            row["value"] = float(val)

        self.advantages, self.returns = compute_gae_for_rows(
            self.rows,
            gamma=gamma,
            gae_lambda=gae_lambda,
        )

        if hasattr(self, "tensors"):
            delattr(self, "tensors")


def cache_metadata(
    source_path: Path,
    gamma: float,
    gae_lambda: float,
    policy_id: str | None,
) -> dict[str, object]:
    stat = source_path.stat()
    return {
        "format": "arena_trajectory_tensor_cache_v1",
        "source_path": str(source_path.resolve()),
        "source_mtime_ns": stat.st_mtime_ns,
        "source_size": stat.st_size,
        "gamma": float(gamma),
        "gae_lambda": float(gae_lambda),
        "policy_id": policy_id,
    }


def load_or_build_tensor_cache(
    source_path: Path,
    cache_path: Path,
    gamma: float,
    gae_lambda: float,
    policy_id: str | None,
) -> dict[str, object]:
    metadata = cache_metadata(source_path, gamma, gae_lambda, policy_id)
    if cache_path.exists():
        try:
            cache = torch.load(cache_path, map_location="cpu", weights_only=False)
            if isinstance(cache, dict) and cache.get("metadata") == metadata:
                return cache
        except Exception:
            pass
    return build_tensor_cache(
        source_path,
        cache_path,
        gamma=gamma,
        gae_lambda=gae_lambda,
        policy_id=policy_id,
    )


def build_tensor_cache(
    source_path: Path,
    cache_path: Path,
    gamma: float,
    gae_lambda: float,
    policy_id: str | None = None,
) -> dict[str, object]:
    rows = [
        json.loads(line)
        for line in source_path.read_text(encoding="utf-8-sig").splitlines()
        if line.strip()
    ]
    if policy_id is not None:
        rows = [row for row in rows if row.get("policy_id") == policy_id]
    advantages, returns = compute_gae_for_rows(
        rows,
        gamma=gamma,
        gae_lambda=gae_lambda,
    )
    tensors = encode_rows(rows, advantages, returns)
    cache = {
        "metadata": cache_metadata(source_path, gamma, gae_lambda, policy_id),
        "rows": rows,
        "advantages": advantages,
        "returns": returns,
        "tensors": tensors,
    }
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(cache, cache_path)
    return cache


def encode_row(row: dict[str, Any], discounted_return: float | None = None) -> dict[str, torch.Tensor]:
    has_global = (
        row.get("global_tile_planes") is not None
        and row.get("global_scalar_features") is not None
    )

    result = {
        "tile_planes": torch.tensor(row["tile_planes"], dtype=torch.float32).view(-1, 34),
        "scalar_features": torch.tensor(row["scalar_features"], dtype=torch.float32),
        "discard_sequence": torch.tensor(
            row["discard_sequence"],
            dtype=torch.float32,
        ).view(DISCARD_SEQUENCE_LENGTH, DISCARD_EVENT_FEATURE_COUNT),
        "discard_mask": torch.tensor(row["discard_mask"], dtype=torch.bool),
        "claim_mask": torch.tensor(row["claim_mask"], dtype=torch.bool),
        "self_kong_mask": torch.tensor(row["self_kong_mask"], dtype=torch.bool),
        "hu_mask": torch.tensor(row["hu_mask"], dtype=torch.bool),
        "action_index": torch.tensor(row["action_index"], dtype=torch.long),
        "reward": torch.tensor(row["reward"], dtype=torch.float32),
        "return": torch.tensor(
            row["reward"] if discounted_return is None else discounted_return,
            dtype=torch.float32,
        ),
        "advantage": torch.tensor(row.get("advantage", 0.0), dtype=torch.float32),
        "step_reward": torch.tensor(row.get("step_reward", 0.0), dtype=torch.float32),
        "terminal_reward": torch.tensor(row.get("terminal_reward", 0.0), dtype=torch.float32),
        "shanten_before": optional_int_tensor(row.get("shanten_before")),
        "shanten_after": optional_int_tensor(row.get("shanten_after")),
        "risk_probs": torch.tensor(row["risk_probs"], dtype=torch.float32),
        "opponent_tenpai_target": torch.tensor(
            row["opponent_tenpai_target"], dtype=torch.float32
        ),
        "opponent_risk_target": torch.tensor(
            row["opponent_risk_target"], dtype=torch.float32
        ),
        "opponent_risk_mask": torch.tensor(
            row["opponent_risk_mask"], dtype=torch.bool
        ),
        "done": torch.tensor(row["done"], dtype=torch.bool),
        "old_log_prob": torch.tensor(row["log_prob"], dtype=torch.float32),
        "old_value": torch.tensor(row["value"], dtype=torch.float32),
        "action_head": torch.tensor(action_head_index(row["action_head"]), dtype=torch.long),
        "has_global_state": torch.tensor(has_global, dtype=torch.bool),
    }

    if has_global:
        result["global_tile_planes"] = torch.tensor(
            row["global_tile_planes"], dtype=torch.float32
        ).view(-1, 34)
        result["global_scalar_features"] = torch.tensor(
            row["global_scalar_features"], dtype=torch.float32
        )

    return result


def encode_rows(
    rows: list[dict[str, Any]],
    advantages: list[float],
    returns: list[float],
) -> dict[str, torch.Tensor]:
    if not rows:
        return empty_tensor_cache()
    encoded = []
    for index, row in enumerate(rows):
        payload = dict(row)
        payload["advantage"] = advantages[index]
        encoded.append(encode_row(payload, returns[index]))
    keys = encoded[0].keys()
    return {
        key: torch.stack([item[key] for item in encoded])
        for key in keys
    }


def empty_tensor_cache() -> dict[str, torch.Tensor]:
    return {
        "tile_planes": torch.empty((0, 10, 34), dtype=torch.float32),
        "scalar_features": torch.empty((0, 12), dtype=torch.float32),
        "discard_sequence": torch.empty(
            (0, DISCARD_SEQUENCE_LENGTH, DISCARD_EVENT_FEATURE_COUNT),
            dtype=torch.float32,
        ),
        "discard_mask": torch.empty((0, 34), dtype=torch.bool),
        "claim_mask": torch.empty((0, 7), dtype=torch.bool),
        "self_kong_mask": torch.empty((0, 3), dtype=torch.bool),
        "hu_mask": torch.empty((0, 2), dtype=torch.bool),
        "action_index": torch.empty((0,), dtype=torch.long),
        "reward": torch.empty((0,), dtype=torch.float32),
        "return": torch.empty((0,), dtype=torch.float32),
        "advantage": torch.empty((0,), dtype=torch.float32),
        "step_reward": torch.empty((0,), dtype=torch.float32),
        "terminal_reward": torch.empty((0,), dtype=torch.float32),
        "shanten_before": torch.empty((0,), dtype=torch.long),
        "shanten_after": torch.empty((0,), dtype=torch.long),
        "risk_probs": torch.empty((0, 34), dtype=torch.float32),
        "opponent_tenpai_target": torch.empty((0, 3), dtype=torch.float32),
        "opponent_risk_target": torch.empty((0, 3, 34), dtype=torch.float32),
        "opponent_risk_mask": torch.empty((0, 3, 34), dtype=torch.bool),
        "done": torch.empty((0,), dtype=torch.bool),
        "old_log_prob": torch.empty((0,), dtype=torch.float32),
        "old_value": torch.empty((0,), dtype=torch.float32),
        "action_head": torch.empty((0,), dtype=torch.long),
        "has_global_state": torch.empty((0,), dtype=torch.bool),
        "global_tile_planes": torch.empty((0, 40, 34), dtype=torch.float32),
        "global_scalar_features": torch.empty((0, 20), dtype=torch.float32),
    }


def action_head_index(action_head: str) -> int:
    mapping = {"discard": 0, "claim": 1, "self_kong": 2, "hu": 3}
    return mapping[action_head]


def optional_int_tensor(value: Any) -> torch.Tensor:
    return torch.tensor(-1 if value is None else int(value), dtype=torch.long)


def trajectory_diagnostics(rows: list[dict[str, Any]]) -> dict[str, float | int]:
    terminal_abs = terminal_abs_sum(rows)
    step_abs = abs_sum(rows, "step_reward")
    diagnostics: dict[str, float | int] = {
        "row_count": len(rows),
        "terminal_reward_mean": terminal_mean_value(rows),
        "step_reward_mean": mean_value(rows, "step_reward"),
        "terminal_reward_abs_sum": terminal_abs,
        "step_reward_abs_sum": step_abs,
        "terminal_step_abs_ratio": terminal_abs / step_abs if step_abs > 0.0 else 0.0,
        "shanten_improvement_count": lower_is_better_improvement_count(
            rows,
            "shanten_before",
            "shanten_after",
        ),
    }
    for action_head in ("discard", "claim", "self_kong", "hu"):
        diagnostics[f"action_head_{action_head}"] = sum(
            1 for row in rows if row.get("action_head") == action_head
        )
    return diagnostics


def mean_value(rows: list[dict[str, Any]], key: str) -> float:
    if not rows:
        return 0.0
    return sum(float(row.get(key, 0.0) or 0.0) for row in rows) / len(rows)


def abs_sum(rows: list[dict[str, Any]], key: str) -> float:
    return sum(abs(float(row.get(key, 0.0) or 0.0)) for row in rows)


def terminal_mean_value(rows: list[dict[str, Any]]) -> float:
    if not rows:
        return 0.0
    return sum(terminal_reward(row) for row in rows) / len(rows)


def terminal_abs_sum(rows: list[dict[str, Any]]) -> float:
    return sum(abs(terminal_reward(row)) for row in rows)


def terminal_reward(row: dict[str, Any]) -> float:
    if not bool(row.get("done")):
        return 0.0
    return float(row.get("terminal_reward", 0.0) or 0.0)


def lower_is_better_improvement_count(
    rows: list[dict[str, Any]],
    before_key: str,
    after_key: str,
) -> int:
    count = 0
    for row in rows:
        before = row.get(before_key)
        after = row.get(after_key)
        if before is not None and after is not None and int(after) < int(before):
            count += 1
    return count


def compute_returns(rewards: list[float], dones: list[bool], gamma: float) -> list[float]:
    returns = [0.0 for _ in rewards]
    running = 0.0
    for index in range(len(rewards) - 1, -1, -1):
        if dones[index]:
            running = 0.0
        running = rewards[index] + gamma * running
        returns[index] = round(running, 6)
    return returns


def compute_discounted_returns_for_rows(
    rows: list[dict[str, Any]],
    gamma: float,
) -> list[float]:
    returns = [0.0 for _ in rows]
    groups: dict[tuple[str, int], list[int]] = {}
    for index, row in enumerate(rows):
        key = (str(row["match_id"]), int(row["seat_index"]))
        groups.setdefault(key, []).append(index)

    for indices in groups.values():
        running = 0.0
        for index in reversed(indices):
            running = float(rows[index]["reward"]) + gamma * running
            returns[index] = round(running, 6)
    return returns


def compute_gae_for_rows(
    rows: list[dict[str, Any]],
    gamma: float,
    gae_lambda: float,
) -> tuple[list[float], list[float]]:
    advantages = [0.0 for _ in rows]
    returns = [0.0 for _ in rows]
    groups: dict[tuple[str, int], list[int]] = {}
    for index, row in enumerate(rows):
        key = (str(row["match_id"]), int(row["seat_index"]))
        groups.setdefault(key, []).append(index)

    for indices in groups.values():
        running_advantage = 0.0
        next_value = 0.0
        for index in reversed(indices):
            row = rows[index]
            reward = compute_shaped_reward(row)
            value = float(row.get("value", 0.0))
            delta = reward + gamma * next_value - value
            running_advantage = delta + gamma * gae_lambda * running_advantage
            advantages[index] = round(running_advantage, 6)
            returns[index] = round(value + running_advantage, 6)
            next_value = value
    return advantages, returns


def compute_shaped_reward(row: dict[str, Any]) -> float:
    # Arena already writes step and terminal shaping into reward.
    return float(row["reward"])
