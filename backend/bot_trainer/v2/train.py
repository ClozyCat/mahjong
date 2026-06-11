from __future__ import annotations

import argparse
import json
import math
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from tqdm import tqdm

try:
    import torch
    import torch.nn.functional as F
    from torch.utils.data import DataLoader
except ModuleNotFoundError as exc:
    raise SystemExit("PyTorch is required: pip install torch") from exc

from dataset import (
    DISCARD_EVENT_FEATURE_COUNT,
    DISCARD_SEQUENCE_LENGTH,
    IGNORE_INDEX,
    MahjongDecisionDataset,
    SCALAR_FEATURE_COUNT,
    TILE_PLANE_COUNT,
)
from model import ModelConfig, build_model

CLAIM_ACTION_NAMES = ["pass", "hu", "pung", "kong", "chow_left", "chow_mid", "chow_right"]
SELF_KONG_ACTION_NAMES = ["pass", "concealed_kong", "add_kong"]
DEFAULT_CLAIM_RARE_ACTION_WEIGHT = 2.0
DEFAULT_SELF_KONG_RARE_ACTION_WEIGHT = 3.0
DEFAULT_HU_POSITIVE_WEIGHT = 3.0
DEFAULT_QUALIFYING_FAN_LOSS_WEIGHT = 0.75

def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    args.output.mkdir(parents=True, exist_ok=True)
    device = resolve_device(args.device)
    
    # 动态判断是否支持 AMP (ROCm 环境下的 "cuda" 支持，DirectML 暂不支持)
    is_rocm_or_cuda = device.type == "cuda"
    use_amp = args.amp and is_rocm_or_cuda
    amp_device_type = "cuda" if is_rocm_or_cuda else "cpu"

    print("Initializing datasets...")
    train_dataset = MahjongDecisionDataset(
        args.data / "train.jsonl",
        args.data / "metadata.json",
        cache_dir=args.data_cache_dir,
        rebuild_cache=args.rebuild_data_cache,
    )
    val_path = args.data / "val.jsonl"
    val_dataset = (
        MahjongDecisionDataset(
            val_path,
            args.data / "metadata.json",
            cache_dir=args.data_cache_dir,
            rebuild_cache=args.rebuild_data_cache,
        )
        if val_path.exists()
        else None
    )

    train_loader = build_loader(train_dataset, args.batch_size, True, args.num_workers, device)
    val_loader = (
        build_loader(val_dataset, args.batch_size, False, args.num_workers, device)
        if val_dataset is not None and len(val_dataset) > 0
        else None
    )

    model_config = model_config_from_args(args)
    model = build_model(model_config).to(device)
    
    # 动态处理模型编译：仅在支持的后端上开启
    if args.compile and hasattr(torch, "compile"):
        if is_rocm_or_cuda:
            model = torch.compile(model)
        else:
            print("Warning: 当前硬件后端 (如 DirectML/CPU) 暂不支持 torch.compile，已跳过。")
            
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=args.weight_decay)
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, T_max=args.epochs, eta_min=args.lr_min)
    scaler = torch.amp.GradScaler(
        amp_device_type,
        enabled=use_amp,
        init_scale=1024.0,
        growth_interval=4000,
    )
    print(f"device={device} amp={use_amp} num_workers={args.num_workers} data_cache={args.data_cache_dir or 'auto'}")
    print(f"Training safeguards: grad_clip={args.grad_clip_norm} nan_check=enabled early_stop_patience={args.early_stop_patience}")
    best_metric = math.inf
    best_metrics: dict[str, float] = {}
    epochs_without_improvement = 0
    nan_count = 0
    
    for epoch in range(1, args.epochs + 1):
        loss_weights = loss_weights_for_epoch(
            epoch=epoch,
            warmup_epochs=args.aux_loss_warmup_epochs,
            claim_weight=args.claim_loss_weight,
            self_kong_weight=args.self_kong_loss_weight,
            hu_weight=args.hu_loss_weight,
            value_start=args.value_loss_start_weight,
            value_target=args.value_loss_weight,
            fan_start=args.fan_loss_start_weight,
            fan_target=args.fan_loss_weight,
            qualifying_fan_start=args.qualifying_fan_loss_start_weight,
            qualifying_fan_target=args.qualifying_fan_loss_weight,
            risk_start=args.risk_loss_start_weight,
            risk_target=args.risk_loss_weight,
        )
        loss_weights["risk_pos_weight"] = args.risk_pos_weight
        loss_weights["claim_rare_action_weight"] = args.claim_rare_action_weight
        loss_weights["self_kong_rare_action_weight"] = args.self_kong_rare_action_weight
        loss_weights["hu_positive_weight"] = args.hu_positive_weight
        current_lr = optimizer.param_groups[0]["lr"]
        train_metrics = run_epoch(
            model, train_loader, optimizer, device, scaler, use_amp,
            loss_weights=loss_weights,
            epoch_desc=f"Train Epoch {epoch}/{args.epochs}",
            grad_clip_norm=args.grad_clip_norm,
        )

        # NaN检测和early stopping
        if math.isnan(train_metrics["loss"]) or math.isinf(train_metrics["loss"]):
            nan_count += 1
            print(f"⚠️  Warning: NaN/Inf detected in training loss (count: {nan_count}/{args.max_nan_tolerance})")
            if nan_count >= args.max_nan_tolerance:
                print(f"❌ Training stopped: NaN/Inf loss exceeded tolerance ({args.max_nan_tolerance} times)")
                break
            # 跳过这个epoch，不更新scheduler
            continue
        else:
            nan_count = 0  # 重置计数器

        scheduler.step()

        val_metrics = (
            run_epoch(
                model, val_loader, None, device, scaler, use_amp,
                loss_weights=loss_weights,
                epoch_desc=f"Val Epoch {epoch}/{args.epochs}",
                grad_clip_norm=None,  # 验证时不需要梯度裁剪
            )
            if val_loader is not None
            else train_metrics
        )

        selection_metric = val_metrics["loss"]
        if selection_metric < best_metric:
            best_metric = selection_metric
            best_metrics = val_metrics
            epochs_without_improvement = 0
            save_checkpoint(
                args.output / "best.pt",
                model,
                train_dataset.metadata,
                val_metrics,
                epoch,
                model_config,
            )
        else:
            epochs_without_improvement += 1
            if args.early_stop_patience > 0 and epochs_without_improvement >= args.early_stop_patience:
                print(f"Early stopping: no improvement for {args.early_stop_patience} epochs")
                break

        print(
            f"Epoch {epoch} Summary: "
            f"lr={current_lr:.2e} | "
            f"train_loss={train_metrics['loss']:.4f} | "
            f"val_loss={val_metrics['loss']:.4f} | "
            f"qfan_loss={val_metrics['qualifying_fan_loss']:.4f} | "
            f"discard_top1={val_metrics['discard_top1']:.4f} | "
            f"discard_top3={val_metrics['discard_top3']:.4f} | "
            f"discard_top5={val_metrics['discard_top5']:.4f} | "
            f"claim_macro_f1={val_metrics['claim_macro_f1']:.4f} | "
            f"hu_precision={val_metrics['hu_precision']:.4f} | "
            f"hu_recall={val_metrics['hu_recall']:.4f} | "
            f"kong_precision={val_metrics['kong_precision']:.4f} | "
            f"kong_recall={val_metrics['kong_recall']:.4f}"
        )

    (args.output / "metrics.json").write_text(
        json.dumps(best_metrics, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=20)
    parser.add_argument("--batch-size", type=int, default=512)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--lr-min", type=float, default=1e-5, help="Minimum learning rate for cosine annealing scheduler")
    parser.add_argument("--weight-decay", type=float, default=1e-4)
    parser.add_argument("--device", default="auto")
    parser.add_argument("--num-workers", type=int, default=0)
    parser.add_argument("--data-cache-dir", type=Path, default=None)
    parser.add_argument("--rebuild-data-cache", action="store_true")
    parser.add_argument("--amp", action="store_true")
    parser.add_argument("--compile", action="store_true")
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--claim-loss-weight", type=float, default=1.0)
    parser.add_argument("--self-kong-loss-weight", type=float, default=1.0)
    parser.add_argument("--hu-loss-weight", type=float, default=1.0)
    parser.add_argument("--value-loss-weight", type=float, default=0.75)
    parser.add_argument("--fan-loss-weight", type=float, default=0.5)
    parser.add_argument("--qualifying-fan-loss-weight", type=float, default=DEFAULT_QUALIFYING_FAN_LOSS_WEIGHT)
    parser.add_argument("--risk-loss-weight", type=float, default=1.0)
    parser.add_argument("--risk-pos-weight", type=float, default=300.0,
        help="Positive class weight for risk BCE loss. Mitigates extreme class imbalance (~0.3%% positive positions).")
    parser.add_argument("--value-loss-start-weight", type=float, default=0.25)
    parser.add_argument("--fan-loss-start-weight", type=float, default=0.1)
    parser.add_argument("--qualifying-fan-loss-start-weight", type=float, default=0.1)
    parser.add_argument("--risk-loss-start-weight", type=float, default=0.25)
    parser.add_argument("--aux-loss-warmup-epochs", type=int, default=4)
    parser.add_argument("--claim-rare-action-weight", type=float, default=DEFAULT_CLAIM_RARE_ACTION_WEIGHT)
    parser.add_argument("--self-kong-rare-action-weight", type=float, default=DEFAULT_SELF_KONG_RARE_ACTION_WEIGHT)
    parser.add_argument("--hu-positive-weight", type=float, default=DEFAULT_HU_POSITIVE_WEIGHT)
    parser.add_argument("--grad-clip-norm", type=float, default=1.0,
        help="Maximum gradient norm for clipping. Set to 0 to disable.")
    parser.add_argument("--max-nan-tolerance", type=int, default=2,
        help="Maximum number of consecutive NaN losses before stopping training.")
    parser.add_argument("--early-stop-patience", type=int, default=0,
        help="Stop training if validation loss doesn't improve for N epochs. 0 to disable.")
    return parser.parse_args()

def model_config_from_args(args: argparse.Namespace) -> ModelConfig:
    return ModelConfig(
        tile_plane_count=TILE_PLANE_COUNT,
        scalar_feature_count=SCALAR_FEATURE_COUNT,
        discard_sequence_length=DISCARD_SEQUENCE_LENGTH,
        discard_event_feature_count=DISCARD_EVENT_FEATURE_COUNT,
    )

def resolve_device(requested: str) -> torch.device:
    if requested == "auto":
        # 1. 优先尝试 ROCm / CUDA
        if torch.cuda.is_available():
            return torch.device("cuda")
        # 2. 尝试 Windows 专属的 AMD 加速库 (DirectML)
        try:
            import torch_directml
            if torch_directml.is_available():
                return torch_directml.device()
        except ImportError:
            pass
        # 3. 都没有则回退 CPU
        return torch.device("cpu")
    
    if requested == "dml":
        try:
            import torch_directml
            return torch_directml.device()
        except ImportError as exc:
            raise SystemExit("torch-directml 未安装。请运行: pip install torch-directml") from exc
            
    return torch.device(requested)


class DatasetBatchCollator:
    def __init__(self, dataset: MahjongDecisionDataset) -> None:
        self.dataset = dataset

    def __call__(self, indices: list[int]) -> dict[str, torch.Tensor]:
        return self.dataset.get_batch(indices)


def build_loader(
    dataset: MahjongDecisionDataset,
    batch_size: int,
    shuffle: bool,
    num_workers: int,
    device: torch.device,
) -> DataLoader:
    kwargs: dict[str, Any] = {
        "batch_size": batch_size,
        "shuffle": shuffle,
        "num_workers": num_workers,
        "collate_fn": DatasetBatchCollator(dataset),
        "pin_memory": device.type == "cuda", # pin_memory 仅适用于真正的 cuda/ROCm 后端
    }
    if num_workers > 0:
        kwargs["persistent_workers"] = True
        kwargs["prefetch_factor"] = 2
    return DataLoader(dataset, **kwargs)

def run_epoch(
    model: torch.nn.Module,
    loader: DataLoader | None,
    optimizer: torch.optim.Optimizer | None,
    device: torch.device,
    scaler: torch.amp.GradScaler,
    use_amp: bool,
    loss_weights: dict[str, float] | None = None,
    epoch_desc: str = "",
    grad_clip_norm: float | None = None,
) -> dict[str, float]:
    if loader is None:
        return empty_metrics()

    is_training = optimizer is not None
    model.train(is_training)
    totals = MetricTotals(device)
    amp_device_type = "cuda" if device.type == "cuda" else "cpu"

    pbar = tqdm(loader, desc=epoch_desc, leave=False, dynamic_ncols=True)
    nan_batch_count = 0
    max_nan_batches = 5  # 单个epoch内最多容忍5个NaN batch

    for i, batch in enumerate(pbar):
        batch = move_batch(batch, device)
        with torch.set_grad_enabled(is_training):
            with torch.amp.autocast(amp_device_type, enabled=use_amp):
                outputs = forward_model(model, batch)
                losses = compute_losses(outputs, batch, **(loss_weights or {}))
            loss = losses["loss"]

            # NaN/Inf检测
            if torch.isnan(loss) or torch.isinf(loss):
                nan_batch_count += 1
                print(f"\n⚠️  Batch {i}: NaN/Inf loss detected, skipping batch ({nan_batch_count}/{max_nan_batches})")
                if nan_batch_count >= max_nan_batches:
                    print(f"❌ Too many NaN batches in this epoch, stopping epoch early")
                    break
                continue

            if is_training:
                optimizer.zero_grad(set_to_none=True)
                scaler.scale(loss).backward()
                scaler.unscale_(optimizer)

                if not gradients_are_finite(model.parameters()):
                    print(f"\n⚠️  Batch {i}: NaN/Inf gradient detected, skipping batch")
                    optimizer.zero_grad(set_to_none=True)
                    scaler.update()
                    continue

                # 梯度裁剪
                if grad_clip_norm is not None and grad_clip_norm > 0:
                    grad_norm = torch.nn.utils.clip_grad_norm_(model.parameters(), grad_clip_norm)

                    # 检测梯度异常
                    if not torch.isfinite(grad_norm):
                        print(f"\n⚠️  Batch {i}: NaN/Inf gradient detected, skipping batch")
                        optimizer.zero_grad(set_to_none=True)
                        scaler.update()  # 必须调用update()重置scaler状态
                        continue

                    if i % 100 == 0 and grad_norm > grad_clip_norm * 0.8:
                        pbar.set_postfix({"loss": f"{loss.item():.4f}", "grad_norm": f"{grad_norm:.2f}"})

                scaler.step(optimizer)
                scaler.update()

        totals.update(outputs, batch, losses)

        if i % 10 == 0:
            pbar.set_postfix({"loss": f"{loss.item():.4f}"})

    return totals.as_metrics()


def gradients_are_finite(parameters) -> bool:
    for param in parameters:
        if param.grad is not None and not torch.isfinite(param.grad).all():
            return False
    return True


def compute_losses(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    claim_weight: float = 1.0,
    self_kong_weight: float = 1.0,
    hu_weight: float = 1.0,
    value_weight: float = 0.25,
    fan_weight: float = 0.1,
    qualifying_fan_weight: float = DEFAULT_QUALIFYING_FAN_LOSS_WEIGHT,
    risk_weight: float = 0.25,
    risk_pos_weight: float = 300.0,
    claim_rare_action_weight: float = DEFAULT_CLAIM_RARE_ACTION_WEIGHT,
    self_kong_rare_action_weight: float = DEFAULT_SELF_KONG_RARE_ACTION_WEIGHT,
    hu_positive_weight: float = DEFAULT_HU_POSITIVE_WEIGHT,
) -> dict[str, torch.Tensor]:
    outputs = sanitize_outputs(outputs)
    discard_loss = masked_cross_entropy(outputs["discard_logits"], batch["discard_mask"], batch["discard_target"])
    claim_loss = masked_cross_entropy(
        outputs["claim_logits"],
        batch["claim_mask"],
        batch["claim_target"],
        non_pass_class_weights(outputs["claim_logits"], claim_rare_action_weight),
    )
    self_kong_loss = masked_cross_entropy(
        outputs["self_kong_logits"],
        batch["self_kong_mask"],
        batch["self_kong_target"],
        non_pass_class_weights(outputs["self_kong_logits"], self_kong_rare_action_weight),
    )
    hu_loss = masked_cross_entropy(
        outputs["hu_logits"],
        batch["hu_mask"],
        batch["hu_target"],
        hu_class_weights(outputs["hu_logits"], hu_positive_weight),
    )

    # 添加数值稳定性保护
    value_loss = F.mse_loss(outputs["value"], batch["value_target"].float())
    value_loss = torch.clamp(value_loss, max=100.0)  # 防止MSE爆炸
    fan_loss = F.mse_loss(outputs["fan_value"], batch["fan_target"].float())
    fan_loss = torch.clamp(fan_loss, max=100.0)
    qualifying_fan_loss = F.mse_loss(
        outputs["qualifying_fan_value"],
        batch["qualifying_fan_target"].float(),
    )
    qualifying_fan_loss = torch.clamp(qualifying_fan_loss, max=100.0)

    risk_loss = masked_binary_cross_entropy_with_logits(
        outputs["risk_logits"],
        batch["risk_target"].float(),
        batch["risk_mask"],
        risk_pos_weight,
    )

    loss = (
        discard_loss
        + claim_weight * claim_loss
        + self_kong_weight * self_kong_loss
        + hu_weight * hu_loss
        + value_weight * value_loss
        + fan_weight * fan_loss
        + qualifying_fan_weight * qualifying_fan_loss
        + risk_weight * risk_loss
    )
    return {
        "loss": loss,
        "discard_loss": discard_loss,
        "claim_loss": claim_loss,
        "self_kong_loss": self_kong_loss,
        "hu_loss": hu_loss,
        "value_loss": value_loss,
        "fan_loss": fan_loss,
        "qualifying_fan_loss": qualifying_fan_loss,
        "risk_loss": risk_loss,
    }


def loss_weights_for_epoch(
    epoch: int,
    warmup_epochs: int,
    claim_weight: float,
    self_kong_weight: float,
    hu_weight: float,
    value_start: float,
    value_target: float,
    fan_start: float,
    fan_target: float,
    qualifying_fan_start: float,
    qualifying_fan_target: float,
    risk_start: float,
    risk_target: float,
) -> dict[str, float]:
    if warmup_epochs <= 1:
        progress = 1.0
    else:
        progress = min(max(epoch - 1, 0), warmup_epochs - 1) / float(warmup_epochs - 1)
    return {
        "claim_weight": claim_weight,
        "self_kong_weight": self_kong_weight,
        "hu_weight": hu_weight,
        "value_weight": value_start + (value_target - value_start) * progress,
        "fan_weight": fan_start + (fan_target - fan_start) * progress,
        "qualifying_fan_weight": qualifying_fan_start
        + (qualifying_fan_target - qualifying_fan_start) * progress,
        "risk_weight": risk_start + (risk_target - risk_start) * progress,
    }

def masked_cross_entropy(
    logits: torch.Tensor,
    mask: torch.Tensor,
    target: torch.Tensor,
    class_weights: torch.Tensor | None = None,
) -> torch.Tensor:
    logits = sanitize_tensor(logits).float()
    active = target != IGNORE_INDEX
    if not torch.any(active):
        return logits.float().sum() * 0.0
    masked_logits = logits.masked_fill(~mask.bool(), -1.0e4)

    # 数值稳定性：裁剪logits防止exp溢出
    masked_logits = torch.clamp(masked_logits, min=-100.0, max=100.0)

    active_targets = target[active].long()
    losses = F.cross_entropy(masked_logits[active], active_targets, reduction="none")
    if class_weights is not None:
        losses = losses * class_weights[active_targets]
    return losses.mean()


def non_pass_class_weights(logits: torch.Tensor, rare_action_weight: float) -> torch.Tensor:
    weights = torch.ones((logits.shape[1],), device=logits.device, dtype=torch.float32)
    if logits.shape[1] > 1:
        weights[1:] = rare_action_weight
    return weights


def hu_class_weights(logits: torch.Tensor, positive_weight: float) -> torch.Tensor:
    weights = torch.ones((logits.shape[1],), device=logits.device, dtype=torch.float32)
    if logits.shape[1] > 1:
        weights[1] = positive_weight
    return weights


def masked_binary_cross_entropy_with_logits(
    logits: torch.Tensor,
    target: torch.Tensor,
    mask: torch.Tensor,
    risk_pos_weight: float,
) -> torch.Tensor:
    logits = sanitize_tensor(logits).float()
    target = target.float()
    active = mask.bool()
    if not torch.any(active):
        return logits.float().sum() * 0.0
    pos_weight = torch.full(
        (logits.shape[1],),
        risk_pos_weight,
        device=logits.device,
    )
    losses = F.binary_cross_entropy_with_logits(
        logits,
        target,
        pos_weight=pos_weight,
        reduction="none",
    )
    return losses[active].mean()


def sanitize_outputs(outputs: dict[str, torch.Tensor]) -> dict[str, torch.Tensor]:
    return {name: sanitize_tensor(tensor).float() for name, tensor in outputs.items()}


def sanitize_tensor(tensor: torch.Tensor) -> torch.Tensor:
    return torch.nan_to_num(tensor, nan=0.0, posinf=100.0, neginf=-100.0)


def forward_model(
    model: torch.nn.Module,
    batch: dict[str, torch.Tensor],
) -> dict[str, torch.Tensor]:
    return model(
        batch["tile_planes"].float(),
        batch["scalar_features"].float(),
        batch["discard_sequence"].float(),
    )

class MetricTotals:
    def __init__(self, device: torch.device) -> None:
        self.device = device
        self.loss_sum = torch.tensor(0.0, device=device)
        self.discard_loss_sum = torch.tensor(0.0, device=device)
        self.claim_loss_sum = torch.tensor(0.0, device=device)
        self.self_kong_loss_sum = torch.tensor(0.0, device=device)
        self.hu_loss_sum = torch.tensor(0.0, device=device)
        self.value_loss_sum = torch.tensor(0.0, device=device)
        self.fan_loss_sum = torch.tensor(0.0, device=device)
        self.qualifying_fan_loss_sum = torch.tensor(0.0, device=device)
        self.risk_loss_sum = torch.tensor(0.0, device=device)
        self.batch_count = 0
        self.discard_top1 = torch.tensor(0, device=device)
        self.discard_top3 = torch.tensor(0, device=device)
        self.discard_top5 = torch.tensor(0, device=device)
        self.discard_count = torch.tensor(0, device=device)
        self.claim_confusion = torch.zeros((7, 7), dtype=torch.int64, device=device)
        self.hu_confusion = torch.zeros((2, 2), dtype=torch.int64, device=device)
        self.kong_confusion = torch.zeros((3, 3), dtype=torch.int64, device=device)

    def update(self, outputs: dict[str, torch.Tensor], batch: dict[str, torch.Tensor], losses: dict[str, torch.Tensor]) -> None:
        self.loss_sum += losses["loss"].detach()
        self.discard_loss_sum += losses["discard_loss"].detach()
        self.claim_loss_sum += losses["claim_loss"].detach()
        self.self_kong_loss_sum += losses["self_kong_loss"].detach()
        self.hu_loss_sum += losses["hu_loss"].detach()
        self.value_loss_sum += losses["value_loss"].detach()
        self.fan_loss_sum += losses["fan_loss"].detach()
        self.qualifying_fan_loss_sum += losses["qualifying_fan_loss"].detach()
        self.risk_loss_sum += losses["risk_loss"].detach()
        self.batch_count += 1
        self.update_discard(outputs["discard_logits"], batch)
        self.update_claim(outputs["claim_logits"], batch)
        self.update_hu(outputs["hu_logits"], batch)
        self.update_kong(outputs["self_kong_logits"], batch)

    def update_discard(self, logits: torch.Tensor, batch: dict[str, torch.Tensor]) -> None:
        target = batch["discard_target"]
        active = target != IGNORE_INDEX
        if not torch.any(active):
            return
        masked = logits.masked_fill(~batch["discard_mask"].bool(), -1.0e4)
        topk = torch.topk(masked[active], k=min(5, masked.shape[1]), dim=1).indices
        target_active = target[active].long()
        self.discard_top1 += (topk[:, 0] == target_active).sum()
        self.discard_top3 += (topk[:, : min(3, topk.shape[1])] == target_active.unsqueeze(1)).any(dim=1).sum()
        self.discard_top5 += (topk == target_active.unsqueeze(1)).any(dim=1).sum()
        self.discard_count += target_active.numel()

    def update_claim(self, logits: torch.Tensor, batch: dict[str, torch.Tensor]) -> None:
        target = batch["claim_target"]
        active = target != IGNORE_INDEX
        if not torch.any(active):
            return
        masked = logits.masked_fill(~batch["claim_mask"].bool(), -1.0e4)
        pred = masked[active].argmax(dim=1)
        target_active = target[active].long()
        indices = target_active * 7 + pred 
        counts = torch.bincount(indices, minlength=49)
        self.claim_confusion += counts.view(7, 7)

    def update_hu(self, logits: torch.Tensor, batch: dict[str, torch.Tensor]) -> None:
        target = batch["hu_target"]
        active = target != IGNORE_INDEX
        if not torch.any(active):
            return
        masked = logits.masked_fill(~batch["hu_mask"].bool(), -1.0e4)
        pred = masked[active].argmax(dim=1)
        target_active = target[active].long()
        indices = target_active * 2 + pred
        counts = torch.bincount(indices, minlength=4)
        self.hu_confusion += counts.view(2, 2)

    def update_kong(self, logits: torch.Tensor, batch: dict[str, torch.Tensor]) -> None:
        target = batch["self_kong_target"]
        active = target != IGNORE_INDEX
        if not torch.any(active):
            return
        masked = logits.masked_fill(~batch["self_kong_mask"].bool(), -1.0e4)
        pred = masked[active].argmax(dim=1)
        target_active = target[active].long()
        indices = target_active * 3 + pred
        counts = torch.bincount(indices, minlength=9)
        self.kong_confusion += counts.view(3, 3)

    def as_metrics(self) -> dict[str, float]:
        claim_f1 = macro_f1(self.claim_confusion)
        hu_precision, hu_recall = positive_precision_recall(self.hu_confusion, 1)
        kong_precision, kong_recall = grouped_positive_precision_recall(self.kong_confusion)
        metrics = {
            "loss": self.loss_sum.item() / max(1, self.batch_count),
            "discard_loss": self.discard_loss_sum.item() / max(1, self.batch_count),
            "claim_loss": self.claim_loss_sum.item() / max(1, self.batch_count),
            "self_kong_loss": self.self_kong_loss_sum.item() / max(1, self.batch_count),
            "hu_loss": self.hu_loss_sum.item() / max(1, self.batch_count),
            "value_loss": self.value_loss_sum.item() / max(1, self.batch_count),
            "fan_loss": self.fan_loss_sum.item() / max(1, self.batch_count),
            "qualifying_fan_loss": self.qualifying_fan_loss_sum.item() / max(1, self.batch_count),
            "risk_loss": self.risk_loss_sum.item() / max(1, self.batch_count),
            "discard_top1": self.discard_top1.item() / max(1, self.discard_count.item()),
            "discard_top3": self.discard_top3.item() / max(1, self.discard_count.item()),
            "discard_top5": self.discard_top5.item() / max(1, self.discard_count.item()),
            "claim_macro_f1": claim_f1,
            "hu_precision": hu_precision,
            "hu_recall": hu_recall,
            "kong_precision": kong_precision,
            "kong_recall": kong_recall,
        }
        metrics.update(per_class_metrics(self.claim_confusion, CLAIM_ACTION_NAMES, "claim"))
        metrics.update(per_class_metrics(self.kong_confusion, SELF_KONG_ACTION_NAMES, "self_kong"))
        return metrics

def macro_f1(confusion: torch.Tensor) -> float:
    scores = []
    for index in range(confusion.shape[0]):
        tp = float(confusion[index, index].item())
        fp = float((confusion[:, index].sum() - confusion[index, index]).item())
        fn = float((confusion[index, :].sum() - confusion[index, index]).item())
        if tp + fp + fn == 0.0:
            continue
        precision = tp / max(1.0, tp + fp)
        recall = tp / max(1.0, tp + fn)
        scores.append(0.0 if precision + recall == 0.0 else 2 * precision * recall / (precision + recall))
    return sum(scores) / max(1, len(scores))

def positive_precision_recall(confusion: torch.Tensor, index: int) -> tuple[float, float]:
    tp = float(confusion[index, index].item())
    fp = float((confusion[:, index].sum() - confusion[index, index]).item())
    fn = float((confusion[index, :].sum() - confusion[index, index]).item())
    return tp / max(1.0, tp + fp), tp / max(1.0, tp + fn)

def grouped_positive_precision_recall(confusion: torch.Tensor) -> tuple[float, float]:
    tp = float(confusion[1:, 1:].sum().item())
    fp = float(confusion[0, 1:].sum().item())
    fn = float(confusion[1:, 0].sum().item())
    return tp / max(1.0, tp + fp), tp / max(1.0, tp + fn)

def per_class_metrics(confusion: torch.Tensor, names: list[str], prefix: str) -> dict[str, float]:
    metrics: dict[str, float] = {}
    for index, name in enumerate(names):
        if index >= confusion.shape[0]:
            continue
        tp = float(confusion[index, index].item())
        fp = float((confusion[:, index].sum() - confusion[index, index]).item())
        fn = float((confusion[index, :].sum() - confusion[index, index]).item())
        support = float(confusion[index, :].sum().item())
        precision = tp / max(1.0, tp + fp)
        recall = tp / max(1.0, tp + fn)
        f1 = 0.0 if precision + recall == 0.0 else 2 * precision * recall / (precision + recall)
        metrics[f"{prefix}_{name}_precision"] = precision
        metrics[f"{prefix}_{name}_recall"] = recall
        metrics[f"{prefix}_{name}_f1"] = f1
        metrics[f"{prefix}_{name}_support"] = support
    return metrics

def empty_metrics() -> dict[str, float]:
    metrics = {
        "loss": 0.0,
        "discard_loss": 0.0,
        "claim_loss": 0.0,
        "self_kong_loss": 0.0,
        "hu_loss": 0.0,
        "value_loss": 0.0,
        "fan_loss": 0.0,
        "qualifying_fan_loss": 0.0,
        "risk_loss": 0.0,
        "discard_top1": 0.0,
        "discard_top3": 0.0,
        "discard_top5": 0.0,
        "claim_macro_f1": 0.0,
        "hu_precision": 0.0,
        "hu_recall": 0.0,
        "kong_precision": 0.0,
        "kong_recall": 0.0,
    }
    for name in CLAIM_ACTION_NAMES:
        metrics[f"claim_{name}_precision"] = 0.0
        metrics[f"claim_{name}_recall"] = 0.0
        metrics[f"claim_{name}_f1"] = 0.0
        metrics[f"claim_{name}_support"] = 0.0
    for name in SELF_KONG_ACTION_NAMES:
        metrics[f"self_kong_{name}_precision"] = 0.0
        metrics[f"self_kong_{name}_recall"] = 0.0
        metrics[f"self_kong_{name}_f1"] = 0.0
        metrics[f"self_kong_{name}_support"] = 0.0
    return metrics

def move_batch(batch: dict[str, torch.Tensor], device: torch.device) -> dict[str, torch.Tensor]:
    return {key: value.to(device, non_blocking=True) for key, value in batch.items()}

def save_checkpoint(
    path: Path,
    model: torch.nn.Module,
    metadata: dict[str, Any],
    metrics: dict[str, float],
    epoch: int,
    model_config: ModelConfig,
) -> None:
    torch.save(
        {
            "model_state": model.state_dict(),
            "metadata": metadata,
            "metrics": metrics,
            "epoch": epoch,
            "model_config": model_config.to_dict(),
            "training_source": "sft",
            "created_at_utc": datetime.now(UTC).isoformat(),
        },
        path,
    )

if __name__ == "__main__":
    main()
