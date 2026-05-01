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
    suited_block_count: int = 2
    honor_block_count: int = 1
    use_se: bool = False
    se_reduction: int = 8
    film_scalar: bool = False
    use_discard_sequence: bool = False

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> "ModelConfig":
        return cls(
            tile_plane_count=int(value.get("tile_plane_count", 10)),
            scalar_feature_count=int(value.get("scalar_feature_count", 10)),
            suited_block_count=int(value.get("suited_block_count", 2)),
            honor_block_count=int(value.get("honor_block_count", 1)),
            use_se=bool(value.get("use_se", False)),
            se_reduction=int(value.get("se_reduction", 8)),
            film_scalar=bool(value.get("film_scalar", False)),
            use_discard_sequence=bool(value.get("use_discard_sequence", False)),
        )

    def to_dict(self) -> dict[str, int | bool]:
        return {
            "tile_plane_count": self.tile_plane_count,
            "scalar_feature_count": self.scalar_feature_count,
            "suited_block_count": self.suited_block_count,
            "honor_block_count": self.honor_block_count,
            "use_se": self.use_se,
            "se_reduction": self.se_reduction,
            "film_scalar": self.film_scalar,
            "use_discard_sequence": self.use_discard_sequence,
        }


if nn is not None:

    class SEBlock(nn.Module):
        def __init__(self, channels: int, reduction: int = 8) -> None:
            super().__init__()
            hidden_channels = max(1, channels // reduction)
            self.net = nn.Sequential(
                nn.AdaptiveAvgPool1d(1),
                nn.Conv1d(channels, hidden_channels, kernel_size=1),
                nn.ReLU(),
                nn.Conv1d(hidden_channels, channels, kernel_size=1),
                nn.Sigmoid(),
            )

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            return x * self.net(x)


    class ResidualConvBlock(nn.Module):
        def __init__(
            self,
            channels: int,
            use_se: bool = False,
            se_reduction: int = 8,
        ) -> None:
            super().__init__()
            self.net = nn.Sequential(
                nn.Conv1d(channels, channels, kernel_size=3, padding=1),
                nn.ReLU(),
                nn.Conv1d(channels, channels, kernel_size=3, padding=1),
            )
            self.norm = nn.BatchNorm1d(channels)
            self.se = SEBlock(channels, se_reduction) if use_se else nn.Identity()

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            residual = self.se(self.norm(self.net(x)))
            return torch.relu(residual + x)


    class SuitAwareTileResNet(nn.Module):
        def __init__(
            self,
            tile_plane_count: int,
            embedding_size: int = 512,
            channels: int = 64,
            suited_block_count: int = 2,
            honor_block_count: int = 1,
            use_se: bool = False,
            se_reduction: int = 8,
        ) -> None:
            super().__init__()
            self.suited_encoder = self._make_encoder(
                tile_plane_count,
                channels,
                suited_block_count,
                use_se,
                se_reduction,
            )
            self.honor_encoder = self._make_encoder(
                tile_plane_count,
                channels,
                honor_block_count,
                use_se,
                se_reduction,
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
            use_se: bool,
            se_reduction: int,
        ) -> nn.Sequential:
            layers: list[nn.Module] = [
                nn.Conv1d(tile_plane_count, channels, kernel_size=3, padding=1),
                nn.ReLU(),
            ]
            layers.extend(
                ResidualConvBlock(channels, use_se=use_se, se_reduction=se_reduction)
                for _ in range(block_count)
            )
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
                suited_block_count=config.suited_block_count,
                honor_block_count=config.honor_block_count,
                use_se=config.use_se,
                se_reduction=config.se_reduction,
            )
            self.scalar_film = (
                nn.Sequential(
                    nn.Linear(config.scalar_feature_count, 128),
                    nn.ReLU(),
                    nn.Linear(128, 1024),
                )
                if config.film_scalar
                else None
            )
            if self.scalar_film is not None:
                nn.init.zeros_(self.scalar_film[-1].weight)
                nn.init.zeros_(self.scalar_film[-1].bias)
            self.scalar_encoder = nn.Sequential(
                nn.Linear(config.scalar_feature_count, 128),
                nn.ReLU(),
                nn.LayerNorm(128),
            )
            self.sequence_encoder = (
                nn.GRU(
                    input_size=38,
                    hidden_size=64,
                    num_layers=1,
                    batch_first=True,
                )
                if config.use_discard_sequence
                else None
            )
            trunk_input_size = 640 + (64 if config.use_discard_sequence else 0)
            self.trunk = nn.Sequential(
                nn.Linear(trunk_input_size, 512),
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
            discard_sequence: torch.Tensor | None = None,
        ) -> dict[str, torch.Tensor]:
            tile_embedding = self.tile_encoder(tile_planes)
            if self.scalar_film is not None:
                gamma, beta = self.scalar_film(scalar_features).chunk(2, dim=1)
                tile_embedding = tile_embedding * (1.0 + gamma) + beta
            scalar_embedding = self.scalar_encoder(scalar_features)
            embeddings = [tile_embedding, scalar_embedding]
            if self.sequence_encoder is not None:
                if discard_sequence is None:
                    discard_sequence = torch.zeros(
                        (tile_planes.shape[0], 64, 38),
                        dtype=tile_planes.dtype,
                        device=tile_planes.device,
                    )
                _, hidden_state = self.sequence_encoder(discard_sequence.float())
                embeddings.append(hidden_state[-1])
            hidden = self.trunk(torch.cat(embeddings, dim=1))
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
