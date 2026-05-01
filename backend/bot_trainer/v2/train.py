from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any

from tqdm import tqdm

try:
    import torch
    import torch.nn.functional as F
    from torch.utils.data import DataLoader
except ModuleNotFoundError as exc:
    raise SystemExit("PyTorch is required: pip install torch") from exc

from dataset import IGNORE_INDEX, MahjongDecisionDataset, SCALAR_FEATURE_COUNT, TILE_PLANE_COUNT
from model import ModelConfig, build_model

CLAIM_ACTION_NAMES = ["pass", "hu", "pung", "kong", "chow_left", "chow_mid", "chow_right"]
SELF_KONG_ACTION_NAMES = ["pass", "concealed_kong", "add_kong"]

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
    scaler = torch.amp.GradScaler(amp_device_type, enabled=use_amp)
    print(f"device={device} amp={use_amp} num_workers={args.num_workers} data_cache={args.data_cache_dir or 'auto'}")
    loss_weights = {
        "claim_weight": args.claim_loss_weight,
        "self_kong_weight": args.self_kong_loss_weight,
        "hu_weight": args.hu_loss_weight,
        "value_weight": args.value_loss_weight,
        "risk_weight": args.risk_loss_weight,
    }

    best_metric = math.inf
    best_metrics: dict[str, float] = {}
    
    for epoch in range(1, args.epochs + 1):
        train_metrics = run_epoch(
            model, train_loader, optimizer, device, scaler, use_amp, 
            loss_weights=loss_weights,
            epoch_desc=f"Train Epoch {epoch}/{args.epochs}"
        )
        
        val_metrics = (
            run_epoch(
                model, val_loader, None, device, scaler, use_amp, 
                loss_weights=loss_weights,
                epoch_desc=f"Val Epoch {epoch}/{args.epochs}"
            )
            if val_loader is not None
            else train_metrics
        )
        
        selection_metric = val_metrics["loss"]
        if selection_metric < best_metric:
            best_metric = selection_metric
            best_metrics = val_metrics
            save_checkpoint(
                args.output / "best.pt",
                model,
                train_dataset.metadata,
                val_metrics,
                epoch,
                model_config,
            )

        print(
            f"Epoch {epoch} Summary: "
            f"train_loss={train_metrics['loss']:.4f} | "
            f"val_loss={val_metrics['loss']:.4f} | "
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
    parser.add_argument("--lr", type=float, default=1e-3)
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
    parser.add_argument("--value-loss-weight", type=float, default=0.25)
    parser.add_argument("--risk-loss-weight", type=float, default=0.25)
    parser.add_argument("--suited-block-count", type=int, default=2)
    parser.add_argument("--honor-block-count", type=int, default=1)
    parser.add_argument("--use-se", action="store_true")
    parser.add_argument("--se-reduction", type=int, default=8)
    parser.add_argument("--film-scalar", action="store_true")
    parser.add_argument("--use-discard-sequence", action="store_true")
    return parser.parse_args()

def model_config_from_args(args: argparse.Namespace) -> ModelConfig:
    return ModelConfig(
        tile_plane_count=TILE_PLANE_COUNT,
        scalar_feature_count=SCALAR_FEATURE_COUNT,
        suited_block_count=args.suited_block_count,
        honor_block_count=args.honor_block_count,
        use_se=args.use_se,
        se_reduction=args.se_reduction,
        film_scalar=args.film_scalar,
        use_discard_sequence=args.use_discard_sequence,
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
) -> dict[str, float]:
    if loader is None:
        return empty_metrics()

    is_training = optimizer is not None
    model.train(is_training)
    totals = MetricTotals(device)
    amp_device_type = "cuda" if device.type == "cuda" else "cpu"

    pbar = tqdm(loader, desc=epoch_desc, leave=False, dynamic_ncols=True)

    for i, batch in enumerate(pbar):
        batch = move_batch(batch, device)
        with torch.set_grad_enabled(is_training):
            with torch.amp.autocast(amp_device_type, enabled=use_amp):
                outputs = forward_model(model, batch)
                losses = compute_losses(outputs, batch, **(loss_weights or {}))
            loss = losses["loss"]
            if is_training:
                optimizer.zero_grad(set_to_none=True)
                scaler.scale(loss).backward()
                scaler.step(optimizer)
                scaler.update()
        
        totals.update(outputs, batch, losses)
        
        if i % 10 == 0:
            pbar.set_postfix({"loss": f"{loss.item():.4f}"})

    return totals.as_metrics()

def compute_losses(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    claim_weight: float = 1.0,
    self_kong_weight: float = 1.0,
    hu_weight: float = 1.0,
    value_weight: float = 0.25,
    risk_weight: float = 0.25,
) -> dict[str, torch.Tensor]:
    discard_loss = masked_cross_entropy(outputs["discard_logits"], batch["discard_mask"], batch["discard_target"])
    claim_loss = masked_cross_entropy(outputs["claim_logits"], batch["claim_mask"], batch["claim_target"])
    self_kong_loss = masked_cross_entropy(outputs["self_kong_logits"], batch["self_kong_mask"], batch["self_kong_target"])
    hu_loss = masked_cross_entropy(outputs["hu_logits"], batch["hu_mask"], batch["hu_target"])
    value_loss = F.mse_loss(outputs["value"], batch["value_target"].float())
    risk_loss = F.binary_cross_entropy_with_logits(outputs["risk_logits"], batch["risk_target"].float())
    loss = (
        discard_loss
        + claim_weight * claim_loss
        + self_kong_weight * self_kong_loss
        + hu_weight * hu_loss
        + value_weight * value_loss
        + risk_weight * risk_loss
    )
    return {
        "loss": loss,
        "discard_loss": discard_loss,
        "claim_loss": claim_loss,
        "self_kong_loss": self_kong_loss,
        "hu_loss": hu_loss,
        "value_loss": value_loss,
        "risk_loss": risk_loss,
    }

def masked_cross_entropy(logits: torch.Tensor, mask: torch.Tensor, target: torch.Tensor) -> torch.Tensor:
    active = target != IGNORE_INDEX
    if not torch.any(active):
        return logits.sum() * 0.0
    masked_logits = logits.masked_fill(~mask.bool(), -1.0e4)
    return F.cross_entropy(masked_logits[active], target[active].long())


def forward_model(
    model: torch.nn.Module,
    batch: dict[str, torch.Tensor],
) -> dict[str, torch.Tensor]:
    config = getattr(model, "config", None)
    if config is None and hasattr(model, "_orig_mod"):
        config = getattr(model._orig_mod, "config", None)
    if getattr(config, "use_discard_sequence", False):
        return model(
            batch["tile_planes"].float(),
            batch["scalar_features"].float(),
            batch["discard_sequence"].float(),
        )
    return model(batch["tile_planes"].float(), batch["scalar_features"].float())

class MetricTotals:
    def __init__(self, device: torch.device) -> None:
        self.device = device
        self.loss_sum = torch.tensor(0.0, device=device)
        self.discard_loss_sum = torch.tensor(0.0, device=device)
        self.claim_loss_sum = torch.tensor(0.0, device=device)
        self.self_kong_loss_sum = torch.tensor(0.0, device=device)
        self.hu_loss_sum = torch.tensor(0.0, device=device)
        self.value_loss_sum = torch.tensor(0.0, device=device)
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
        },
        path,
    )

if __name__ == "__main__":
    main()
