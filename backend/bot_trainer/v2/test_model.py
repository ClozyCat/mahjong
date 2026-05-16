from __future__ import annotations

import torch

import model as model_module
from model import ModelConfig, build_model


def parameter_count(model: torch.nn.Module) -> int:
    return sum(parameter.numel() for parameter in model.parameters())


def sequence_aware_config() -> ModelConfig:
    return ModelConfig(
        tile_plane_count=10,
        scalar_feature_count=12,
        discard_sequence_length=32,
        discard_event_feature_count=40,
    )


def test_sequence_aware_model_output_shapes() -> None:
    model = build_model(sequence_aware_config())
    outputs = model(
        torch.zeros((2, 10, 34)),
        torch.zeros((2, 12)),
        torch.zeros((2, 32, 40)),
    )

    assert outputs["discard_logits"].shape == (2, 34)
    assert outputs["claim_logits"].shape == (2, 7)
    assert outputs["self_kong_logits"].shape == (2, 3)
    assert outputs["hu_logits"].shape == (2, 2)
    assert outputs["value"].shape == (2, 1)
    assert outputs["risk_logits"].shape == (2, 34)
    assert outputs["fan_logits"].shape == (2, 1)


def test_sequence_aware_model_uses_dropout_with_correct_rate() -> None:
    model = build_model(sequence_aware_config())

    dropouts = [m for m in model.modules() if isinstance(m, torch.nn.Dropout)]
    assert len(dropouts) > 0
    assert all(d.p == 0.15 for d in dropouts)


def test_legacy_compatible_loader_is_removed() -> None:
    assert not hasattr(model_module, "load_compatible_state_dict")


def test_model_config_from_dict_reads_sequence_schema() -> None:
    config = ModelConfig.from_dict(
        {
            "tile_plane_count": 10,
            "scalar_feature_count": 12,
            "discard_sequence_length": 32,
            "discard_event_feature_count": 40,
        }
    )

    assert config.tile_plane_count == 10
    assert config.scalar_feature_count == 12
    assert config.discard_sequence_length == 32
    assert config.discard_event_feature_count == 40
