import torch
from torch import nn
from model import (
    ModelConfig, LightweightActor, build_model,
    ResidualConvBlock, GRUDiscardSequenceEncoder,
    SuitFusionTileEncoder, HeadMLP, OpponentModelingHead,
)


class TestLightweightActor:
    def test_forward_shapes(self):
        m = build_model(ModelConfig())
        m.eval()
        tp = torch.zeros((2, 10, 34))
        sf = torch.zeros((2, 12))
        ds = torch.zeros((2, 32, 40))
        out = m(tp, sf, ds)
        assert out["discard_logits"].shape == (2, 34)
        assert out["claim_logits"].shape == (2, 7)
        assert out["self_kong_logits"].shape == (2, 3)
        assert out["hu_logits"].shape == (2, 2)
        assert out["value_for_risk"].shape == (2, 1)
        assert out["fan_value"].shape == (2, 1)
        assert out["qualifying_fan_value"].shape == (2, 1)
        assert out["value"].shape == (2, 1)
        assert out["opponent_tenpai_logits"].shape == (2, 3)
        assert out["opponent_risk_logits"].shape == (2, 3, 34)

    def test_model_config_roundtrip(self):
        cfg = ModelConfig()
        d = cfg.to_dict()
        cfg2 = ModelConfig.from_dict(d)
        assert cfg == cfg2

    def test_build_model_param_count(self):
        m = build_model(ModelConfig())
        total = sum(p.numel() for p in m.parameters())
        assert total < 3_000_000, f"Model too large: {total}"

    def test_training_heads_separate(self):
        m = build_model(ModelConfig())
        assert "value" not in LightweightActor.ONNX_OUTPUT_NAMES
        assert "value" in m.TRAINING_ONLY_HEADS

    def test_value_for_risk_reuses_trained_value_head(self):
        m = build_model(ModelConfig())
        m.train()
        tp = torch.randn((2, 10, 34))
        sf = torch.randn((2, 12))
        ds = torch.randn((2, 32, 40))

        out = m(tp, sf, ds)

        assert "value_for_risk_head" not in dict(m.named_modules())
        assert out["value_for_risk"].data_ptr() == out["value"].data_ptr()

        out["value_for_risk"].sum().backward()

        value_head_grads = [
            param.grad
            for name, param in m.named_parameters()
            if name.startswith("value_head.")
        ]
        assert value_head_grads
        assert all(grad is not None for grad in value_head_grads)

    def test_gradient_flow(self):
        m = build_model(ModelConfig())
        m.train()
        tp = torch.randn((2, 10, 34))
        sf = torch.randn((2, 12))
        ds = torch.randn((2, 32, 40))
        out = m(tp, sf, ds)
        loss = (
            out["discard_logits"].sum()
            + out["value"].sum()
            + out["score_bucket_logits"].sum()
            + out["opponent_tenpai_logits"].sum()
            + out["opponent_risk_logits"].sum()
        )
        loss.backward()
        grad_count = 0
        for name, param in m.named_parameters():
            if param.grad is None:
                continue
            assert torch.isfinite(param.grad).all(), f"Non-finite grad for {name}"
            grad_count += 1
        assert grad_count > 0, "No parameters received gradients"

    def test_score_bucket_head_present(self):
        m = build_model(ModelConfig())
        m.eval()
        tp = torch.zeros((2, 10, 34))
        sf = torch.zeros((2, 12))
        ds = torch.zeros((2, 32, 40))
        out = m(tp, sf, ds)
        assert "score_bucket_logits" in out
        assert out["score_bucket_logits"].shape == (2, 5)

    def test_score_bucket_head_not_in_onnx(self):
        m = build_model(ModelConfig())
        assert "score_bucket_logits" not in LightweightActor.ONNX_OUTPUT_NAMES
        assert "score_bucket_logits" in m.TRAINING_ONLY_HEADS


class TestGRUEncoder:
    def test_output_shape(self):
        enc = GRUDiscardSequenceEncoder(40, 192, 96)
        x = torch.zeros((2, 32, 40))
        out = enc(x)
        assert out.shape == (2, 192)

    def test_batch_independence(self):
        enc = GRUDiscardSequenceEncoder(40, 192, 96)
        enc.eval()
        x1 = torch.randn((4, 32, 40))
        x2 = x1.clone()
        out1 = enc(x1)
        out2 = enc(x2)
        assert torch.allclose(out1, out2)


class TestSuitFusionTileEncoder:
    def test_output_shape(self):
        enc = SuitFusionTileEncoder(10, embedding_size=256, channels=64)
        x = torch.zeros((2, 10, 34))
        out = enc(x)
        assert out.shape == (2, 256)

    def test_shared_backbone(self):
        backbone = nn.Sequential(
            nn.Conv1d(10, 64, 3, padding=1), nn.ReLU(), ResidualConvBlock(64)
        )
        enc = SuitFusionTileEncoder(
            10, embedding_size=256, channels=64, shared_backbone=backbone
        )
        x = torch.zeros((2, 10, 34))
        out = enc(x)
        assert out.shape == (2, 256)
