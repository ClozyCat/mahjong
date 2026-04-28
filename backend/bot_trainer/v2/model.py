from __future__ import annotations

from typing import NamedTuple

try:
    import torch
    from torch import nn
except ModuleNotFoundError:  # pragma: no cover
    torch = None
    nn = None


class MissingTorchError(RuntimeError):
    pass


class ModelConfig(NamedTuple):
    tile_plane_count: int
    scalar_feature_count: int


if nn is not None:

    class MahjongPolicyNetV2(nn.Module):
        def __init__(self, tile_plane_count: int, scalar_feature_count: int) -> None:
            super().__init__()
            self.tile_plane_count = tile_plane_count
            self.scalar_feature_count = scalar_feature_count
            self.tile_encoder = nn.Sequential(
                nn.Flatten(),
                nn.Linear(tile_plane_count * 34, 512),
                nn.ReLU(),
                nn.LayerNorm(512),
            )
            self.scalar_encoder = nn.Sequential(
                nn.Linear(scalar_feature_count, 128),
                nn.ReLU(),
                nn.LayerNorm(128),
            )
            self.trunk = nn.Sequential(
                nn.Linear(640, 512),
                nn.ReLU(),
                nn.Dropout(0.1),
                nn.Linear(512, 256),
                nn.ReLU(),
            )
            self.discard_head = nn.Linear(256, 34)
            self.claim_head = nn.Linear(256, 7)
            self.self_kong_head = nn.Linear(256, 3)
            self.hu_head = nn.Linear(256, 2)
            self.value_head = nn.Linear(256, 1)
            self.risk_head = nn.Linear(256, 34)

        def forward(
            self,
            tile_planes: torch.Tensor,
            scalar_features: torch.Tensor,
        ) -> dict[str, torch.Tensor]:
            tile_embedding = self.tile_encoder(tile_planes)
            scalar_embedding = self.scalar_encoder(scalar_features)
            hidden = self.trunk(torch.cat([tile_embedding, scalar_embedding], dim=1))
            return {
                "discard_logits": self.discard_head(hidden),
                "claim_logits": self.claim_head(hidden),
                "self_kong_logits": self.self_kong_head(hidden),
                "hu_logits": self.hu_head(hidden),
                "value": self.value_head(hidden),
                "risk_logits": self.risk_head(hidden),
            }

else:

    class MahjongPolicyNetV2:  # type: ignore[no-redef]
        def __init__(self, *_args: object, **_kwargs: object) -> None:
            raise MissingTorchError("PyTorch is required: pip install torch")


def build_model(config: ModelConfig) -> MahjongPolicyNetV2:
    return MahjongPolicyNetV2(config.tile_plane_count, config.scalar_feature_count)
