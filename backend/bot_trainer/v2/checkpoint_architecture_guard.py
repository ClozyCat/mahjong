from __future__ import annotations

import argparse
from pathlib import Path

from rl_train import validate_checkpoint_architecture


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--use-actor-critic", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    validate_checkpoint_architecture(args.checkpoint, args.use_actor_critic)


if __name__ == "__main__":
    main()
