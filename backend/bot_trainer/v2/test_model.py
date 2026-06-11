from __future__ import annotations

import os
import sys
import types

import torch

import model as model_module
from model import ModelConfig, build_actor_critic, build_model


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
    assert outputs["fan_value"].shape == (2, 1)
    assert outputs["qualifying_fan_value"].shape == (2, 1)
    assert outputs["risk_logits"].shape == (2, 34)


def test_rl_forward_model_ignores_global_features_for_shared_policy() -> None:
    from rl_train import forward_model

    model = build_model(sequence_aware_config())
    batch = {
        "tile_planes": torch.zeros((2, 10, 34)),
        "scalar_features": torch.zeros((2, 12)),
        "discard_sequence": torch.zeros((2, 32, 40)),
        "has_global_state": torch.tensor([True, True]),
        "global_tile_planes": torch.zeros((2, 40, 34)),
        "global_scalar_features": torch.zeros((2, 20)),
    }

    outputs = forward_model(model, batch)

    assert outputs["discard_logits"].shape == (2, 34)
    assert outputs["value"].shape == (2, 1)


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


def test_bootstrap_actor_critic_checkpoint_from_shared_policy(tmp_path) -> None:
    from bootstrap_actor_critic_checkpoint import bootstrap_actor_critic_checkpoint

    config = sequence_aware_config()
    shared = build_model(config)
    with torch.no_grad():
        shared.policy_trunk[0].weight.fill_(0.25)
        shared.fan_head.net[0].weight.fill_(0.5)
        shared.qualifying_fan_head.net[0].weight.fill_(0.75)
    source = tmp_path / "sft.pt"
    output = tmp_path / "actor_critic.pt"
    torch.save(
        {
            "model_state": shared.state_dict(),
            "model_config": config.to_dict(),
            "training_source": "sft",
        },
        source,
    )

    manifest = bootstrap_actor_critic_checkpoint(source, output)

    payload = torch.load(output, map_location="cpu")
    state = payload["model_state"]
    assert any(key.startswith("actor.") for key in state)
    assert any(key.startswith("critic.") for key in state)
    assert torch.equal(
        state["actor.policy_trunk.0.weight"],
        shared.state_dict()["policy_trunk.0.weight"],
    )
    assert torch.equal(
        state["actor.fan_head.net.0.weight"],
        shared.state_dict()["fan_head.net.0.weight"],
    )
    assert torch.equal(
        state["actor.qualifying_fan_head.net.0.weight"],
        shared.state_dict()["qualifying_fan_head.net.0.weight"],
    )
    assert payload["training_source"] == "actor_critic_bootstrap"
    assert manifest["copied_actor_keys"] > 0

    actor_critic = build_actor_critic(config)
    missing, unexpected = actor_critic.load_state_dict(state, strict=True)
    assert missing == []
    assert unexpected == []


def test_actor_critic_export_wrapper_preserves_onnx_outputs(tmp_path) -> None:
    from bootstrap_actor_critic_checkpoint import bootstrap_actor_critic_checkpoint
    from export_onnx import OUTPUT_NAMES, OnnxWrapper, load_export_model

    config = sequence_aware_config()
    shared = build_model(config)
    source = tmp_path / "sft.pt"
    checkpoint = tmp_path / "actor_critic.pt"
    torch.save(
        {
            "model_state": shared.state_dict(),
            "model_config": config.to_dict(),
            "training_source": "sft",
        },
        source,
    )
    bootstrap_actor_critic_checkpoint(source, checkpoint)

    model, model_config, is_actor_critic = load_export_model(checkpoint)
    wrapper = OnnxWrapper(model)
    outputs = wrapper(
        torch.zeros((2, model_config.tile_plane_count, 34)),
        torch.zeros((2, model_config.scalar_feature_count)),
        torch.zeros(
            (
                2,
                model_config.discard_sequence_length,
                model_config.discard_event_feature_count,
            )
        ),
    )

    assert is_actor_critic
    assert len(outputs) == len(OUTPUT_NAMES)
    assert outputs[OUTPUT_NAMES.index("discard_logits")].shape == (2, 34)
    assert outputs[OUTPUT_NAMES.index("value")].shape == (2, 1)
    assert outputs[OUTPUT_NAMES.index("fan_value")].shape == (2, 1)
    assert outputs[OUTPUT_NAMES.index("qualifying_fan_value")].shape == (2, 1)


def test_quantize_onnx_rebuilds_stale_existing_quantized_model(tmp_path, monkeypatch) -> None:
    import export_onnx

    fp32_path = tmp_path / "sft.onnx"
    fp32_path.write_bytes(b"fp32")
    quant_path = tmp_path / "sft.quant.onnx"
    quant_path.write_text("stale", encoding="utf-8")

    def fake_output_names(path):
        if path == quant_path:
            return ["discard_logits"]
        return export_onnx.OUTPUT_NAMES

    def fake_quant_pre_process(source, output):
        assert source == fp32_path.as_posix()
        torch.save({"source": source}, output)

    def fake_quantize_dynamic(source, output, **kwargs):
        assert "preprocessed.onnx" in source
        quant_path.write_text("rebuilt", encoding="utf-8")

    onnxruntime = types.ModuleType("onnxruntime")
    onnxruntime.__path__ = []
    quantization = types.ModuleType("onnxruntime.quantization")
    quantization.QuantType = types.SimpleNamespace(QInt8="qint8")
    quantization.quantize_dynamic = fake_quantize_dynamic
    shape_inference = types.ModuleType("onnxruntime.quantization.shape_inference")
    shape_inference.quant_pre_process = fake_quant_pre_process
    onnxruntime.quantization = quantization

    monkeypatch.setattr(export_onnx, "onnx_output_names", fake_output_names)
    monkeypatch.setitem(sys.modules, "onnxruntime", onnxruntime)
    monkeypatch.setitem(sys.modules, "onnxruntime.quantization", quantization)
    monkeypatch.setitem(sys.modules, "onnxruntime.quantization.shape_inference", shape_inference)

    result = export_onnx.quantize_onnx(fp32_path)

    assert result == quant_path
    assert quant_path.read_text(encoding="utf-8") == "rebuilt"


def test_quantize_onnx_rebuilds_outdated_existing_quantized_model(tmp_path, monkeypatch) -> None:
    import export_onnx

    fp32_path = tmp_path / "sft.onnx"
    fp32_path.write_bytes(b"fp32")
    quant_path = tmp_path / "sft.quant.onnx"
    quant_path.write_text("old weights", encoding="utf-8")
    os.utime(quant_path, (1000, 1000))
    os.utime(fp32_path, (2000, 2000))

    def fake_quant_pre_process(source, output):
        assert source == fp32_path.as_posix()
        torch.save({"source": source}, output)

    def fake_quantize_dynamic(source, output, **kwargs):
        assert "preprocessed.onnx" in source
        quant_path.write_text("new weights", encoding="utf-8")

    onnxruntime = types.ModuleType("onnxruntime")
    onnxruntime.__path__ = []
    quantization = types.ModuleType("onnxruntime.quantization")
    quantization.QuantType = types.SimpleNamespace(QInt8="qint8")
    quantization.quantize_dynamic = fake_quantize_dynamic
    shape_inference = types.ModuleType("onnxruntime.quantization.shape_inference")
    shape_inference.quant_pre_process = fake_quant_pre_process
    onnxruntime.quantization = quantization

    monkeypatch.setattr(export_onnx, "onnx_output_names", lambda path: export_onnx.OUTPUT_NAMES)
    monkeypatch.setitem(sys.modules, "onnxruntime", onnxruntime)
    monkeypatch.setitem(sys.modules, "onnxruntime.quantization", quantization)
    monkeypatch.setitem(sys.modules, "onnxruntime.quantization.shape_inference", shape_inference)

    result = export_onnx.quantize_onnx(fp32_path)

    assert result == quant_path
    assert quant_path.read_text(encoding="utf-8") == "new weights"
