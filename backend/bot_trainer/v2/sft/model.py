from __future__ import annotations

from typing import NamedTuple

try:
    import torch
    import torch.nn.functional as F
    from torch import nn
except ModuleNotFoundError:  # pragma: no cover
    torch = None
    nn = None


class MissingTorchError(RuntimeError):
    pass


class ModelConfig(NamedTuple):
    tile_plane_count: int = 10
    scalar_feature_count: int = 13
    discard_sequence_length: int = 32
    discard_event_feature_count: int = 40

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> "ModelConfig":
        return cls(
            tile_plane_count=int(value.get("tile_plane_count", 10)),
            scalar_feature_count=int(value.get("scalar_feature_count", 13)),
            discard_sequence_length=int(value.get("discard_sequence_length", 32)),
            discard_event_feature_count=int(value.get("discard_event_feature_count", 40)),
        )

    def to_dict(self) -> dict[str, int]:
        return {
            "tile_plane_count": self.tile_plane_count,
            "scalar_feature_count": self.scalar_feature_count,
            "discard_sequence_length": self.discard_sequence_length,
            "discard_event_feature_count": self.discard_event_feature_count,
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
            self.norm = nn.GroupNorm(1, channels)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            return torch.relu(self.norm(self.net(x)) + x)


    class GroupAttention(nn.Module):
        def __init__(self, channels: int, num_heads: int = 4) -> None:
            super().__init__()
            self.norm = nn.LayerNorm(channels)
            self.attn = nn.MultiheadAttention(channels, num_heads, batch_first=True)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            attended, _ = self.attn(self.norm(x), self.norm(x), self.norm(x))
            return x + attended


    class SuitFusionTileEncoder(nn.Module):
        def __init__(
            self,
            tile_plane_count: int,
            embedding_size: int = 256,
            channels: int = 64,
            use_attention: bool = False,
            shared_backbone: nn.Module | None = None,
        ) -> None:
            super().__init__()
            self.use_attention = use_attention
            self.use_shared = shared_backbone is not None
            self.embedding_size = embedding_size

            if self.use_shared:
                self.shared = shared_backbone
                self.suit_residuals = nn.ModuleList([ResidualConvBlock(channels) for _ in range(2)])
                self.honor_residuals = nn.ModuleList([ResidualConvBlock(channels) for _ in range(1)])
                self.suit_pool = nn.Sequential(nn.AdaptiveAvgPool1d(1), nn.Flatten())
                self.honor_pool = nn.Sequential(nn.AdaptiveAvgPool1d(1), nn.Flatten())
            else:
                self.suited_encoder = self._make_encoder(tile_plane_count, channels, 3)
                self.honor_encoder = self._make_encoder(tile_plane_count, channels, 2)

            if use_attention:
                self.group_attention = GroupAttention(channels)
            self.fusion = nn.Sequential(
                nn.Linear(channels * 4, channels * 4),
                nn.ReLU(),
                nn.LayerNorm(channels * 4),
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
            if self.use_shared:
                suit_embeddings = []
                for start in (0, 9, 18):
                    x = self.shared(tile_planes[:, :, start : start + 9])
                    for block in self.suit_residuals:
                        x = block(x)
                    suit_embeddings.append(self.suit_pool(x))
                x = self.shared(tile_planes[:, :, 27:34])
                for block in self.honor_residuals:
                    x = block(x)
                honor_embedding = self.honor_pool(x)
            else:
                suit_embeddings = [
                    self.suited_encoder(tile_planes[:, :, start : start + 9])
                    for start in (0, 9, 18)
                ]
                honor_embedding = self.honor_encoder(tile_planes[:, :, 27:34])
            if self.use_attention:
                group_embeds = torch.stack(
                    [*suit_embeddings, honor_embedding], dim=1
                )
                group_embeds = self.group_attention(group_embeds)
                combined = group_embeds.reshape(group_embeds.size(0), -1)
            else:
                combined = torch.cat([*suit_embeddings, honor_embedding], dim=1)
            return self.fusion(combined)


    class GRUDiscardSequenceEncoder(nn.Module):
        def __init__(
            self,
            event_feature_count: int,
            embedding_size: int = 192,
            hidden_size: int = 96,
            num_layers: int = 1,
        ) -> None:
            super().__init__()
            self.event_projection = nn.Sequential(
                nn.Linear(event_feature_count, hidden_size),
                nn.ReLU(),
                nn.LayerNorm(hidden_size),
            )
            self.gru = nn.GRU(
                hidden_size, hidden_size,
                num_layers=num_layers,
                batch_first=True,
            )
            self.output = nn.Sequential(
                nn.Linear(hidden_size, embedding_size),
                nn.ReLU(),
                nn.LayerNorm(embedding_size),
            )

        def forward(self, discard_sequence: torch.Tensor) -> torch.Tensor:
            projected = self.event_projection(discard_sequence)
            _, hidden = self.gru(projected)
            return self.output(hidden[-1])


    class HeadMLP(nn.Module):
        def __init__(self, input_size: int, output_size: int, hidden_size: int = 256) -> None:
            super().__init__()
            self.net = nn.Sequential(
                nn.Linear(input_size, hidden_size),
                nn.ReLU(),
                nn.Linear(hidden_size, output_size),
            )

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            return self.net(x)


    class OpponentModelingHead(nn.Module):
        def __init__(self, input_size: int, num_opponents: int = 3) -> None:
            super().__init__()
            self.num_opponents = num_opponents
            self.tenpai_head = nn.Linear(input_size, num_opponents)
            self.risk_head = nn.Linear(input_size, 34 * num_opponents)

        def forward(self, x: torch.Tensor) -> dict[str, torch.Tensor]:
            tenpai_logits = self.tenpai_head(x)
            risk_logits = self.risk_head(x).view(-1, self.num_opponents, 34)
            return {
                "opponent_tenpai_logits": tenpai_logits,
                "opponent_risk_logits": risk_logits,
            }


    class LightweightActor(nn.Module):
        """Single model: local features → all heads. No global critic, no actor-critic split."""

        ONNX_OUTPUT_NAMES = [
            "discard_logits",
            "claim_logits",
            "self_kong_logits",
            "hu_logits",
            "value_for_risk",
            "fan_value",
            "qualifying_fan_value",
            "opponent_tenpai_logits",
            "opponent_risk_logits",
        ]

        TRAINING_ONLY_HEADS = {"value", "score_bucket_logits"}

        def __init__(self, config: ModelConfig) -> None:
            super().__init__()
            self.config = config
            self.tile_plane_count = config.tile_plane_count
            self.scalar_feature_count = config.scalar_feature_count
            self.discard_sequence_length = config.discard_sequence_length
            self.discard_event_feature_count = config.discard_event_feature_count

            shared_backbone = self._make_shared_backbone(config.tile_plane_count)
            self.policy_tile_encoder = SuitFusionTileEncoder(
                config.tile_plane_count,
                embedding_size=256,
                channels=64,
                shared_backbone=shared_backbone,
            )
            self.risk_tile_encoder = SuitFusionTileEncoder(
                config.tile_plane_count,
                embedding_size=256,
                channels=64,
                use_attention=True,
                shared_backbone=shared_backbone,
            )
            self.scalar_encoder = nn.Sequential(
                nn.Linear(config.scalar_feature_count, 96),
                nn.ReLU(),
                nn.LayerNorm(96),
            )
            self.discard_sequence_encoder = GRUDiscardSequenceEncoder(
                config.discard_event_feature_count,
                embedding_size=192,
                hidden_size=96,
            )

            combined_size = 256 + 96 + 192
            self.policy_trunk = self._make_trunk(combined_size, 512, 256)
            self.risk_trunk = self._make_trunk(combined_size, 384, 256)

            self.discard_head = HeadMLP(256, 34)
            self.claim_head = HeadMLP(256, 7)
            self.self_kong_head = HeadMLP(256, 3)
            self.hu_head = HeadMLP(256, 2)
            self.fan_head = HeadMLP(256, 1)
            self.qualifying_fan_head = HeadMLP(256, 1)
            self.opponent_modeling = OpponentModelingHead(256, num_opponents=3)

            self.value_head = HeadMLP(256, 1)
            self.score_bucket_head = HeadMLP(256, 5)

        @staticmethod
        def _make_shared_backbone(
            tile_plane_count: int,
            channels: int = 64,
        ) -> nn.Sequential:
            return nn.Sequential(
                nn.Conv1d(tile_plane_count, channels, kernel_size=3, padding=1),
                nn.ReLU(),
                ResidualConvBlock(channels),
            )

        @staticmethod
        def _make_trunk(
            input_size: int,
            hidden_size: int,
            output_size: int,
        ) -> nn.Sequential:
            return nn.Sequential(
                nn.Linear(input_size, hidden_size),
                nn.ReLU(),
                nn.Dropout(0.15),
                nn.LayerNorm(hidden_size),
                nn.Linear(hidden_size, output_size),
                nn.ReLU(),
                nn.Dropout(0.15),
                nn.LayerNorm(output_size),
            )

        def forward(
            self,
            tile_planes: torch.Tensor,
            scalar_features: torch.Tensor,
            discard_sequence: torch.Tensor,
        ) -> dict[str, torch.Tensor]:
            scalar_embedding = self.scalar_encoder(scalar_features)
            sequence_embedding = self.discard_sequence_encoder(discard_sequence)

            policy_features = torch.cat(
                [
                    self.policy_tile_encoder(tile_planes),
                    scalar_embedding,
                    sequence_embedding,
                ],
                dim=1,
            )
            risk_features = torch.cat(
                [
                    self.risk_tile_encoder(tile_planes),
                    scalar_embedding,
                    sequence_embedding,
                ],
                dim=1,
            )

            policy_hidden = self.policy_trunk(policy_features)
            risk_hidden = self.risk_trunk(risk_features)
            opponent_outputs = self.opponent_modeling(risk_hidden)
            value = self.value_head(policy_hidden)

            return {
                "discard_logits": self.discard_head(policy_hidden),
                "claim_logits": self.claim_head(policy_hidden),
                "self_kong_logits": self.self_kong_head(policy_hidden),
                "hu_logits": self.hu_head(policy_hidden),
                "value_for_risk": value,
                "fan_value": self.fan_head(policy_hidden),
                "qualifying_fan_value": self.qualifying_fan_head(policy_hidden),
                "value": value,
                "score_bucket_logits": self.score_bucket_head(policy_hidden),
                **opponent_outputs,
            }

else:

    class LightweightActor:  # type: ignore[no-redef]
        def __init__(self, *_args: object, **_kwargs: object) -> None:
            raise MissingTorchError("PyTorch is required: pip install torch")


def build_model(config: ModelConfig) -> LightweightActor:
    return LightweightActor(config)
