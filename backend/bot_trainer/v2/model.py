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

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> "ModelConfig":
        return cls(
            tile_plane_count=int(value.get("tile_plane_count", 10)),
            scalar_feature_count=int(value.get("scalar_feature_count", 10)),
        )

    def to_dict(self) -> dict[str, int | bool]:
        return {
            "tile_plane_count": self.tile_plane_count,
            "scalar_feature_count": self.scalar_feature_count,
        }


if nn is not None:

    class ResidualConvBlock(nn.Module):
        def __init__(self, channels: int) -> None:
            super().__init__()
            self.net = nn.Sequential(
                nn.Conv1d(channels, channels, kernel_size=3, padding=1),
                nn.ReLU(),
                nn.Conv1d(channels, channels, kernel_size=3, padding=1),
            )
            self.norm = nn.BatchNorm1d(channels)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            residual = self.norm(self.net(x))
            return torch.relu(residual + x)


    class SuitAwareTileResNet(nn.Module):
        def __init__(
            self,
            tile_plane_count: int,
            embedding_size: int = 512,
            channels: int = 64,
        ) -> None:
            super().__init__()
            self.suited_encoder = self._make_encoder(
                tile_plane_count,
                channels,
                block_count=2,
            )
            self.honor_encoder = self._make_encoder(
                tile_plane_count,
                channels,
                block_count=1,
            )
            self.projector = nn.Sequential(
                nn.Linear(channels * 4, embedding_size),
                nn.ReLU(),
                nn.LayerNorm(embedding_size),
            )

        @staticmethod
        def _make_encoder(
            tile_plane_count: int,
            channels: int,
            block_count: int,
        ) -> nn.Sequential:
            layers: list[nn.Module] = [
                nn.Conv1d(tile_plane_count, channels, kernel_size=3, padding=1),
                nn.ReLU(),
            ]
            layers.extend(ResidualConvBlock(channels) for _ in range(block_count))
            layers.extend([nn.AdaptiveAvgPool1d(1), nn.Flatten()])
            return nn.Sequential(*layers)

        def forward(self, tile_planes: torch.Tensor) -> torch.Tensor:
            suit_embeddings = [
                self.suited_encoder(tile_planes[:, :, start : start + 9])
                for start in (0, 9, 18)
            ]
            honor_embedding = self.honor_encoder(tile_planes[:, :, 27:34])
            return self.projector(torch.cat([*suit_embeddings, honor_embedding], dim=1))


    class MahjongPolicyNetV2(nn.Module):
        def __init__(self, config: ModelConfig) -> None:
            super().__init__()
            self.config = config
            self.tile_plane_count = config.tile_plane_count
            self.scalar_feature_count = config.scalar_feature_count
            self.tile_encoder = SuitAwareTileResNet(
                config.tile_plane_count,
                embedding_size=512,
            )
            self.scalar_encoder = nn.Sequential(
                nn.Linear(config.scalar_feature_count, 128),
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
            self.fan_head = nn.Linear(256, 1)

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
                "fan_logits": self.fan_head(hidden),
            }

else:

    class MahjongPolicyNetV2:  # type: ignore[no-redef]
        def __init__(self, *_args: object, **_kwargs: object) -> None:
            raise MissingTorchError("PyTorch is required: pip install torch")


def build_model(config: ModelConfig) -> MahjongPolicyNetV2:
    return MahjongPolicyNetV2(config)


def load_compatible_state_dict(
    model: torch.nn.Module,
    state: dict[str, torch.Tensor],
) -> list[str]:
    current = model.state_dict()
    compatible = {
        key: value
        for key, value in state.items()
        if key in current and current[key].shape == value.shape
    }
    model.load_state_dict(compatible, strict=False)
    return sorted(set(state) - set(compatible))
