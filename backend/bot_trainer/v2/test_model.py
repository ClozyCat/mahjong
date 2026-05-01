from __future__ import annotations

import torch

from model import ModelConfig, build_model, load_compatible_state_dict


def parameter_count(model: torch.nn.Module) -> int:
    return sum(parameter.numel() for parameter in model.parameters())


def test_resnet_model_output_shapes() -> None:
    model = build_model(ModelConfig(tile_plane_count=10, scalar_feature_count=10))
    outputs = model(torch.zeros((2, 10, 34)), torch.zeros((2, 10)))

    assert outputs["discard_logits"].shape == (2, 34)
    assert outputs["claim_logits"].shape == (2, 7)
    assert outputs["self_kong_logits"].shape == (2, 3)
    assert outputs["hu_logits"].shape == (2, 2)
    assert outputs["value"].shape == (2, 1)
    assert outputs["risk_logits"].shape == (2, 34)


def test_compatible_loader_skips_old_tile_encoder_tensors() -> None:
    model = build_model(ModelConfig(tile_plane_count=10, scalar_feature_count=10))
    state = model.state_dict()
    old_state = {
        key: value.clone()
        for key, value in state.items()
        if not key.startswith("tile_encoder.")
    }
    old_state["tile_encoder.1.weight"] = torch.zeros((512, 340))
    old_state["tile_encoder.1.bias"] = torch.zeros((512,))

    skipped = load_compatible_state_dict(model, old_state)

    assert "tile_encoder.1.weight" in skipped
    assert "tile_encoder.1.bias" in skipped
    assert "discard_head.weight" not in skipped


def test_model_config_from_dict_defaults_old_checkpoints() -> None:
    config = ModelConfig.from_dict(
        {
            "tile_plane_count": 10,
            "scalar_feature_count": 10,
        }
    )

    assert config.tile_plane_count == 10
    assert config.scalar_feature_count == 10
