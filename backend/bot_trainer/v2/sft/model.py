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
    tile_plane_count: int = 24
    scalar_feature_count: int = 18
    discard_sequence_length: int = 32
    discard_event_feature_count: int = 40
    channels: int = 96

    @classmethod
    def from_dict(cls, value: dict[str, object]) -> "ModelConfig":
        return cls(
            tile_plane_count=int(value.get("tile_plane_count", 24)),
            scalar_feature_count=int(value.get("scalar_feature_count", 18)),
            discard_sequence_length=int(value.get("discard_sequence_length", 32)),
            discard_event_feature_count=int(value.get("discard_event_feature_count", 40)),
            channels=int(value.get("channels", 96)),
        )

    def to_dict(self) -> dict[str, int]:
        return {
            "tile_plane_count": self.tile_plane_count,
            "scalar_feature_count": self.scalar_feature_count,
            "discard_sequence_length": self.discard_sequence_length,
            "discard_event_feature_count": self.discard_event_feature_count,
            "channels": self.channels,
        }


if nn is not None:

    class ResidualConvBlock(nn.Module):
        def __init__(self, channels: int) -> None:
            super().__init__()
            self.net = nn.Sequential(
                nn.Conv1d(channels, channels, kernel_size=3, padding=1),
                nn.SiLU(),
                nn.Conv1d(channels, channels, kernel_size=3, padding=1),
            )
            self.norm = nn.GroupNorm(1, channels)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            return F.silu(self.norm(self.net(x)) + x)


    class ResidualPointwiseBlock(nn.Module):
        """1x1 residual block for honor tiles (no bogus spatial adjacency between winds/dragons)."""

        def __init__(self, channels: int) -> None:
            super().__init__()
            self.net = nn.Sequential(
                nn.Conv1d(channels, channels, kernel_size=1),
                nn.SiLU(),
                nn.Conv1d(channels, channels, kernel_size=1),
            )
            self.norm = nn.GroupNorm(1, channels)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            return F.silu(self.norm(self.net(x)) + x)


    class GroupAttention(nn.Module):
        def __init__(self, channels: int, num_heads: int = 4) -> None:
            super().__init__()
            self.norm = nn.LayerNorm(channels)
            self.attn = nn.MultiheadAttention(channels, num_heads, batch_first=True)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            normed = self.norm(x)
            attended, _ = self.attn(normed, normed, normed, need_weights=False)
            return x + attended


    class CrossSuitRankFusion(nn.Module):
        """Fuses identical ranks (1..9) across Wan/Tiao/Bing for Guobiao multi-suit fans
        (e.g. Mixed Triple Chow, Mixed Straight, Mixed Shifted Chows/Pungs)."""

        def __init__(self, channels: int) -> None:
            super().__init__()
            self.norm = nn.LayerNorm(channels * 3)
            self.mlp = nn.Sequential(
                nn.Linear(channels * 3, channels),
                nn.SiLU(),
                nn.Linear(channels, channels * 3),
            )

        def forward(self, suit_maps: list[torch.Tensor]) -> list[torch.Tensor]:
            # Each suit_map is [B, C, 9] -> stack to [B, 9, 3*C]
            stacked = torch.cat([m.transpose(1, 2) for m in suit_maps], dim=-1)
            fused = stacked + self.mlp(self.norm(stacked))
            c = suit_maps[0].size(1)
            return [
                fused[:, :, 0:c].transpose(1, 2),
                fused[:, :, c : 2 * c].transpose(1, 2),
                fused[:, :, 2 * c : 3 * c].transpose(1, 2),
            ]


    class SuitFusionTileEncoder(nn.Module):
        """Preserves full [B, C, 34] per-tile spatial features AND produces a global [B, E] summary."""

        def __init__(
            self,
            tile_plane_count: int,
            embedding_size: int = 256,
            channels: int = 96,
            use_attention: bool = True,
            shared_backbone: nn.Module | None = None,
            shared_honor_backbone: nn.Module | None = None,
        ) -> None:
            super().__init__()
            self.channels = channels
            self.embedding_size = embedding_size
            self.use_attention = use_attention

            if shared_backbone is not None:
                self.suit_stem = shared_backbone
            else:
                self.suit_stem = nn.Sequential(
                    nn.Conv1d(tile_plane_count, channels, kernel_size=3, padding=1),
                    nn.SiLU(),
                    ResidualConvBlock(channels),
                )
            if shared_honor_backbone is not None:
                self.honor_stem = shared_honor_backbone
            else:
                self.honor_stem = nn.Sequential(
                    nn.Conv1d(tile_plane_count, channels, kernel_size=1),
                    nn.SiLU(),
                    ResidualPointwiseBlock(channels),
                )

            self.suit_residuals = nn.ModuleList([ResidualConvBlock(channels) for _ in range(2)])
            self.cross_suit_fusion = CrossSuitRankFusion(channels)
            self.honor_residual = ResidualPointwiseBlock(channels)
            self.honor_attention = GroupAttention(channels, num_heads=4)
            self.tile_pos_embed = nn.Parameter(torch.zeros(1, channels, 34))

            if use_attention:
                self.group_attention = GroupAttention(channels * 2, num_heads=4)

            self.fusion = nn.Sequential(
                nn.Linear(channels * 8, channels * 3),
                nn.SiLU(),
                nn.LayerNorm(channels * 3),
                nn.Linear(channels * 3, embedding_size),
                nn.SiLU(),
                nn.LayerNorm(embedding_size),
            )

        def forward_with_map(self, tile_planes: torch.Tensor) -> tuple[torch.Tensor, torch.Tensor]:
            suit_maps: list[torch.Tensor] = []
            for start in (0, 9, 18):
                x = self.suit_stem(tile_planes[:, :, start : start + 9])
                for block in self.suit_residuals:
                    x = block(x)
                suit_maps.append(x)
            suit_maps = self.cross_suit_fusion(suit_maps)

            h = self.honor_stem(tile_planes[:, :, 27:34])
            h = self.honor_residual(h)
            h = self.honor_attention(h.transpose(1, 2)).transpose(1, 2)

            tile_map = torch.cat([*suit_maps, h], dim=2) + self.tile_pos_embed

            group_summaries = [
                torch.cat([m.mean(dim=-1), m.amax(dim=-1)], dim=1)
                for m in (*suit_maps, h)
            ]
            if self.use_attention:
                group_stack = torch.stack(group_summaries, dim=1)
                group_stack = self.group_attention(group_stack)
                combined = group_stack.reshape(group_stack.size(0), -1)
            else:
                combined = torch.cat(group_summaries, dim=1)

            global_embedding = self.fusion(combined)
            return global_embedding, tile_map

        def forward(self, tile_planes: torch.Tensor) -> torch.Tensor:
            global_embedding, _ = self.forward_with_map(tile_planes)
            return global_embedding


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
                nn.SiLU(),
                nn.LayerNorm(hidden_size),
            )
            self.gru = nn.GRU(
                hidden_size,
                hidden_size,
                num_layers=num_layers,
                batch_first=True,
            )
            self.output = nn.Sequential(
                nn.Linear(hidden_size, embedding_size),
                nn.SiLU(),
                nn.LayerNorm(embedding_size),
            )

        def forward(self, discard_sequence: torch.Tensor) -> torch.Tensor:
            projected = self.event_projection(discard_sequence)
            _, hidden = self.gru(projected)
            return self.output(hidden[-1])


    class HeadMLP(nn.Module):
        def __init__(self, input_size: int, output_size: int, hidden_size: int = 128) -> None:
            super().__init__()
            self.net = nn.Sequential(
                nn.Linear(input_size, hidden_size),
                nn.SiLU(),
                nn.Linear(hidden_size, output_size),
            )

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            return self.net(x)


    class SpatialTileHead(nn.Module):
        """Scores each of the 34 tile slots by combining per-tile features with global context."""

        def __init__(
            self,
            tile_channels: int,
            context_size: int,
            out_channels: int = 1,
        ) -> None:
            super().__init__()
            self.context_proj = nn.Linear(context_size, tile_channels)
            self.tile_conv = nn.Sequential(
                nn.Conv1d(tile_channels * 2, tile_channels, kernel_size=1),
                nn.SiLU(),
                nn.GroupNorm(1, tile_channels),
                nn.Conv1d(tile_channels, out_channels, kernel_size=1),
            )

        def forward(self, tile_map: torch.Tensor, context: torch.Tensor) -> torch.Tensor:
            ctx = self.context_proj(context).unsqueeze(-1).expand(-1, -1, tile_map.size(2))
            fused = torch.cat([tile_map, ctx], dim=1)
            return self.tile_conv(fused)


    class OpponentModelingHead(nn.Module):
        def __init__(
            self,
            context_size: int,
            tile_channels: int = 96,
            num_opponents: int = 3,
        ) -> None:
            super().__init__()
            self.num_opponents = num_opponents
            self.tenpai_head = HeadMLP(context_size, num_opponents, hidden_size=128)
            self.risk_spatial_head = SpatialTileHead(
                tile_channels=tile_channels,
                context_size=context_size,
                out_channels=num_opponents,
            )

        def forward(
            self,
            risk_context: torch.Tensor,
            risk_tile_map: torch.Tensor,
        ) -> dict[str, torch.Tensor]:
            tenpai_logits = self.tenpai_head(risk_context)
            risk_logits = self.risk_spatial_head(risk_tile_map, risk_context)
            return {
                "opponent_tenpai_logits": tenpai_logits,
                "opponent_risk_logits": risk_logits,
            }


    class LightweightActor(nn.Module):
        """Single spatial-aware actor model: local features -> all heads."""

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

        TRAINING_ONLY_HEADS = {"value", "score_bucket_logits", "fan_family_logits"}

        def __init__(self, config: ModelConfig) -> None:
            super().__init__()
            self.config = config
            self.tile_plane_count = config.tile_plane_count
            self.scalar_feature_count = config.scalar_feature_count
            self.discard_sequence_length = config.discard_sequence_length
            self.discard_event_feature_count = config.discard_event_feature_count
            channels = config.channels

            shared_suit_stem = self._make_shared_suit_stem(config.tile_plane_count, channels)
            shared_honor_stem = self._make_shared_honor_stem(config.tile_plane_count, channels)
            self.policy_tile_encoder = SuitFusionTileEncoder(
                config.tile_plane_count,
                embedding_size=256,
                channels=channels,
                use_attention=True,
                shared_backbone=shared_suit_stem,
                shared_honor_backbone=shared_honor_stem,
            )
            self.risk_tile_encoder = SuitFusionTileEncoder(
                config.tile_plane_count,
                embedding_size=256,
                channels=channels,
                use_attention=True,
                shared_backbone=shared_suit_stem,
                shared_honor_backbone=shared_honor_stem,
            )
            self.scalar_encoder = nn.Sequential(
                nn.Linear(config.scalar_feature_count, 96),
                nn.SiLU(),
                nn.LayerNorm(96),
            )
            self.discard_sequence_encoder = GRUDiscardSequenceEncoder(
                config.discard_event_feature_count,
                embedding_size=192,
                hidden_size=96,
            )

            combined_size = 256 + 96 + 192
            self.policy_trunk = self._make_trunk(combined_size, 384, 256)
            self.risk_trunk = self._make_trunk(combined_size, 320, 256)

            self.discard_head = SpatialTileHead(
                tile_channels=channels,
                context_size=256,
                out_channels=1,
            )
            self.claim_head = HeadMLP(256, 7, hidden_size=128)
            self.self_kong_head = HeadMLP(256, 3, hidden_size=96)
            self.hu_head = HeadMLP(256, 2, hidden_size=96)
            self.fan_head = HeadMLP(256, 1, hidden_size=128)
            self.qualifying_fan_head = HeadMLP(256, 1, hidden_size=128)
            self.opponent_modeling = OpponentModelingHead(
                context_size=256,
                tile_channels=channels,
                num_opponents=3,
            )

            self.value_head = HeadMLP(256, 1, hidden_size=128)
            self.score_bucket_head = HeadMLP(256, 5, hidden_size=128)
            self.fan_family_head = HeadMLP(256, 8, hidden_size=128)

        @staticmethod
        def _make_shared_suit_stem(
            tile_plane_count: int,
            channels: int = 96,
        ) -> nn.Sequential:
            return nn.Sequential(
                nn.Conv1d(tile_plane_count, channels, kernel_size=3, padding=1),
                nn.SiLU(),
                ResidualConvBlock(channels),
            )

        @staticmethod
        def _make_shared_honor_stem(
            tile_plane_count: int,
            channels: int = 96,
        ) -> nn.Sequential:
            return nn.Sequential(
                nn.Conv1d(tile_plane_count, channels, kernel_size=1),
                nn.SiLU(),
                ResidualPointwiseBlock(channels),
            )

        @staticmethod
        def _make_trunk(
            input_size: int,
            hidden_size: int,
            output_size: int,
        ) -> nn.Sequential:
            return nn.Sequential(
                nn.Linear(input_size, hidden_size),
                nn.SiLU(),
                nn.Dropout(0.10),
                nn.LayerNorm(hidden_size),
                nn.Linear(hidden_size, output_size),
                nn.SiLU(),
                nn.Dropout(0.10),
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

            policy_global, policy_tile_map = self.policy_tile_encoder.forward_with_map(tile_planes)
            risk_global, risk_tile_map = self.risk_tile_encoder.forward_with_map(tile_planes)

            policy_features = torch.cat(
                [policy_global, scalar_embedding, sequence_embedding],
                dim=1,
            )
            risk_features = torch.cat(
                [risk_global, scalar_embedding, sequence_embedding],
                dim=1,
            )

            policy_hidden = self.policy_trunk(policy_features)
            risk_hidden = self.risk_trunk(risk_features)
            opponent_outputs = self.opponent_modeling(risk_hidden, risk_tile_map)
            value = self.value_head(policy_hidden)
            discard_logits = self.discard_head(policy_tile_map, policy_hidden).squeeze(1)

            return {
                "discard_logits": discard_logits,
                "claim_logits": self.claim_head(policy_hidden),
                "self_kong_logits": self.self_kong_head(policy_hidden),
                "hu_logits": self.hu_head(policy_hidden),
                "value_for_risk": value,
                "fan_value": self.fan_head(policy_hidden),
                "qualifying_fan_value": self.qualifying_fan_head(policy_hidden),
                "value": value,
                "score_bucket_logits": self.score_bucket_head(policy_hidden),
                "fan_family_logits": self.fan_family_head(policy_hidden),
                **opponent_outputs,
            }

else:

    class LightweightActor:  # type: ignore[no-redef]
        def __init__(self, *_args: object, **_kwargs: object) -> None:
            raise MissingTorchError("PyTorch is required: pip install torch")


def build_model(config: ModelConfig) -> LightweightActor:
    return LightweightActor(config)
