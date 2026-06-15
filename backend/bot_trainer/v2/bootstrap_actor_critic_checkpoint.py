from __future__ import annotations

import argparse
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import torch

from model import ModelConfig, build_actor_critic


ACTOR_KEY_PREFIXES = {
    "policy_tile_encoder.": "actor.policy_tile_encoder.",
    "value_tile_encoder.": "actor.value_tile_encoder.",
    "risk_tile_encoder.": "actor.risk_tile_encoder.",
    "scalar_encoder.": "actor.scalar_encoder.",
    "discard_sequence_encoder.": "actor.discard_sequence_encoder.",
    "policy_trunk.": "actor.policy_trunk.",
    "value_trunk.": "actor.value_trunk.",
    "risk_trunk.": "actor.risk_trunk.",
    "discard_head.": "actor.discard_head.",
    "claim_head.": "actor.claim_head.",
    "self_kong_head.": "actor.self_kong_head.",
    "hu_head.": "actor.hu_head.",
    "value_head.": "actor.value_head.",
    "fan_head.": "actor.fan_head.",
    "qualifying_fan_head.": "actor.qualifying_fan_head.",
    "opponent_modeling.": "actor.opponent_modeling.",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def load_payload(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise FileNotFoundError(f"source checkpoint not found: {path}")
    payload = torch.load(path, map_location="cpu")
    if not isinstance(payload, dict) or "model_state" not in payload:
        raise ValueError(f"checkpoint must contain model_state: {path}")
    state = payload["model_state"]
    if any(key.startswith("actor.") or key.startswith("critic.") for key in state):
        raise ValueError(f"source checkpoint is already actor-critic: {path}")
    return payload


def actor_key_for_shared_key(shared_key: str) -> str | None:
    for shared_prefix, actor_prefix in ACTOR_KEY_PREFIXES.items():
        if shared_key.startswith(shared_prefix):
            return actor_prefix + shared_key[len(shared_prefix):]
    return None


def copy_shared_actor_weights(
    target_state: dict[str, torch.Tensor],
    source_state: dict[str, torch.Tensor],
) -> int:
    copied = 0
    for source_key, source_value in source_state.items():
        target_keys = []
        target_key = actor_key_for_shared_key(source_key)
        if target_key is not None:
            target_keys.append(target_key)
        for target_key in target_keys:
            if target_key not in target_state:
                continue
            if target_state[target_key].shape != source_value.shape:
                continue
            target_state[target_key] = source_value.clone()
            copied += 1
    return copied


def bootstrap_actor_critic_checkpoint(source: Path, output: Path) -> dict[str, Any]:
    payload = load_payload(source)
    model_config = ModelConfig.from_dict(payload.get("model_config", {}))
    model = build_actor_critic(model_config)
    actor_critic_state = model.state_dict()
    source_state = payload["model_state"]
    copied_actor_keys = copy_shared_actor_weights(actor_critic_state, source_state)
    if copied_actor_keys == 0:
        raise ValueError(f"no actor weights could be copied from source checkpoint: {source}")

    output.parent.mkdir(parents=True, exist_ok=True)
    manifest = {
        "source_checkpoint": source.as_posix(),
        "output_checkpoint": output.as_posix(),
        "source_training_source": payload.get("training_source", "unknown"),
        "copied_actor_keys": copied_actor_keys,
        "critic_initialized": "fresh",
    }
    torch.save(
        {
            "model_state": actor_critic_state,
            "model_config": model_config.to_dict(),
            "training_source": "actor_critic_bootstrap",
            "created_at_utc": datetime.now(UTC).isoformat(),
            "bootstrap": manifest,
        },
        output,
    )
    return manifest


def main() -> None:
    args = parse_args()
    manifest = bootstrap_actor_critic_checkpoint(args.source, args.output)
    print(
        "Actor-critic bootstrap checkpoint saved: "
        f"{manifest['output_checkpoint']} "
        f"(copied_actor_keys={manifest['copied_actor_keys']}, critic_initialized=fresh)"
    )


if __name__ == "__main__":
    main()
