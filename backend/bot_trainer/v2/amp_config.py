from __future__ import annotations

from dataclasses import dataclass

import torch


@dataclass(frozen=True)
class AmpConfig:
    enabled: bool
    device_type: str
    dtype: torch.dtype | None
    scaler_enabled: bool
    disabled_reason: str | None = None


def resolve_amp_config(device: torch.device, requested_amp: bool) -> AmpConfig:
    device_type = "cuda" if device.type == "cuda" else "cpu"
    if not requested_amp:
        return AmpConfig(
            enabled=False,
            device_type=device_type,
            dtype=None,
            scaler_enabled=False,
        )
    if device.type != "cuda":
        return AmpConfig(
            enabled=False,
            device_type=device_type,
            dtype=None,
            scaler_enabled=False,
            disabled_reason=f"{device.type} backend does not support CUDA BF16 AMP",
        )
    is_bf16_supported = getattr(torch.cuda, "is_bf16_supported", lambda: False)
    if not is_bf16_supported():
        return AmpConfig(
            enabled=False,
            device_type=device_type,
            dtype=None,
            scaler_enabled=False,
            disabled_reason="CUDA device does not support BF16; FP16 AMP is intentionally not used",
        )
    return AmpConfig(
        enabled=True,
        device_type=device_type,
        dtype=torch.bfloat16,
        scaler_enabled=False,
    )


def amp_dtype_name(config: AmpConfig) -> str:
    if not config.enabled:
        return "off"
    if config.dtype == torch.bfloat16:
        return "bf16"
    return str(config.dtype)
