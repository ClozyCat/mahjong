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

def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    args.output.mkdir(parents=True, exist_ok=True)
    device = resolve_device(args.device)
    use_amp = args.amp and device.type == "cuda"

    print("Initializing datasets...")
    train_dataset = MahjongDecisionDataset(args.data / "train.jsonl", args.data / "metadata.json")
    val_path = args.data / "val.jsonl"
    val_dataset = MahjongDecisionDataset(val_path, args.data / "metadata.json") if val_path.exists() else None

    train_loader = build_loader(train_dataset, args.batch_size, True, args.num_workers, device)
    val_loader = (
        build_loader(val_dataset, args.batch_size, False, args.num_workers, device)
        if val_dataset is not None and len(val_dataset) > 0
        else None
    )

    model = build_model(ModelConfig(TILE_PLANE_COUNT, SCALAR_FEATURE_COUNT)).to(device)
    if args.compile and hasattr(torch, "compile"):
        model = torch.compile(model)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=args.weight_decay)
    scaler = torch.amp.GradScaler("cuda", enabled=use_amp)
    print(f"device={device} amp={use_amp} num_workers={args.num_workers}")

    best_metric = math.inf
    best_metrics: dict[str, float] = {}
    
    for epoch in range(1, args.epochs + 1):
        train_metrics = run_epoch(
            model, train_loader, optimizer, device, scaler, use_amp, 
            epoch_desc=f"Train Epoch {epoch}/{args.epochs}"
        )
        
        val_metrics = (
            run_epoch(
                model, val_loader, None, device, scaler, use_amp, 
                epoch_desc=f"Val Epoch {epoch}/{args.epochs}"
            )
            if val_loader is not None
            else train_metrics
        )
        
        selection_metric = val_metrics["loss"]
        if selection_metric < best_metric:
            best_metric = selection_metric
            best_metrics = val_metrics
            save_checkpoint(args.output / "best.pt", model, train_dataset.metadata, val_metrics, epoch)

        print(
            f"Epoch {epoch} Summary: "
            f"train_loss={train_metrics['loss']:.4f} | "
            f"val_loss={val_metrics['loss']:.4f} | "
            f"discard_top1={val_metrics['discard_top1']:.4f} | "
            f"claim_macro_f1={val_metrics['claim_macro_f1']:.4f} | "
            f"hu_recall={val_metrics['hu_recall']:.4f} | "
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
    parser.add_argument("--amp", action="store_true")
    parser.add_argument("--compile", action="store_true")
    parser.add_argument("--seed", type=int, default=7)
    return parser.parse_args()

def resolve_device(requested: str) -> torch.device:
    if requested == "auto":
        return torch.device("cuda" if torch.cuda.is_available() else "cpu")
    device = torch.device(requested)
    if device.type == "cuda" and not torch.cuda.is_available():
        raise SystemExit("CUDA was requested")
    return device

def build_loader(
    dataset: MahjongDecisionDataset,
    batch_size: int,
    shuffle: bool,
    num_workers: int,
    device: torch.device,
) -> DataLoader:
    # [核心优化] 告诉 DataLoader：只给我发索引，我自己用 get_batch 切片组装！
    kwargs: dict[str, Any] = {
        "batch_size": batch_size,
        "shuffle": shuffle,
        "num_workers": num_workers,
        "collate_fn": lambda indices: dataset.get_batch(indices), # 自定义拼装
        "pin_memory": device.type == "cuda",
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
    epoch_desc: str = "",
) -> dict[str, float]:
    if loader is None:
        return empty_metrics()

    is_training = optimizer is not None
    model.train(is_training)
    totals = MetricTotals(device)

    pbar = tqdm(loader, desc=epoch_desc, leave=False, dynamic_ncols=True)

    for i, batch in enumerate(pbar):
        batch = move_batch(batch, device)
        with torch.set_grad_enabled(is_training):
            with torch.amp.autocast("cuda", enabled=use_amp):
                outputs = model(batch["tile_planes"].float(), batch["scalar_features"].float())
                losses = compute_losses(outputs, batch)
            loss = losses["loss"]
            if is_training:
                optimizer.zero_grad(set_to_none=True)
                scaler.scale(loss).backward()
                scaler.step(optimizer)
                scaler.update()
        
        totals.update(outputs, batch, losses)
        
        # 降低刷新频率，减少不必要的 GPU 阻塞
        if i % 10 == 0:
            pbar.set_postfix({"loss": f"{loss.item():.4f}"})

    return totals.as_metrics()

def compute_losses(outputs: dict[str, torch.Tensor], batch: dict[str, torch.Tensor]) -> dict[str, torch.Tensor]:
    discard_loss = masked_cross_entropy(outputs["discard_logits"], batch["discard_mask"], batch["discard_target"])
    claim_loss = masked_cross_entropy(outputs["claim_logits"], batch["claim_mask"], batch["claim_target"])
    self_kong_loss = masked_cross_entropy(outputs["self_kong_logits"], batch["self_kong_mask"], batch["self_kong_target"])
    hu_loss = masked_cross_entropy(outputs["hu_logits"], batch["hu_mask"], batch["hu_target"])
    value_loss = F.mse_loss(outputs["value"], batch["value_target"].float())
    risk_loss = F.binary_cross_entropy_with_logits(outputs["risk_logits"], batch["risk_target"].float())
    loss = discard_loss + claim_loss + self_kong_loss + hu_loss + 0.25 * value_loss + 0.25 * risk_loss
    return {"loss": loss, "value_loss": value_loss}

def masked_cross_entropy(logits: torch.Tensor, mask: torch.Tensor, target: torch.Tensor) -> torch.Tensor:
    active = target != IGNORE_INDEX
    if not torch.any(active):
        return logits.sum() * 0.0
    masked_logits = logits.masked_fill(~mask.bool(), -1.0e4)
    return F.cross_entropy(masked_logits[active], target[active].long())

class MetricTotals:
    def __init__(self, device: torch.device) -> None:
        self.device = device
        # [核心优化] 所有累加器直接建立在 GPU 上，避免循环内的数据搬运
        self.loss_sum = torch.tensor(0.0, device=device)
        self.value_loss_sum = torch.tensor(0.0, device=device)
        self.batch_count = 0
        self.discard_top1 = torch.tensor(0, device=device)
        self.discard_top3 = torch.tensor(0, device=device)
        self.discard_count = torch.tensor(0, device=device)
        self.claim_confusion = torch.zeros((7, 7), dtype=torch.int64, device=device)
        self.hu_tp = torch.tensor(0, device=device)
        self.hu_fn = torch.tensor(0, device=device)
        self.kong_tp = torch.tensor(0, device=device)
        self.kong_fn = torch.tensor(0, device=device)

    def update(self, outputs: dict[str, torch.Tensor], batch: dict[str, torch.Tensor], losses: dict[str, torch.Tensor]) -> None:
        # 全部用 .detach() 保留为张量，不使用 .cpu()
        self.loss_sum += losses["loss"].detach()
        self.value_loss_sum += losses["value_loss"].detach()
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
        topk = torch.topk(masked[active], k=min(3, masked.shape[1]), dim=1).indices
        target_active = target[active].long()
        self.discard_top1 += (topk[:, 0] == target_active).sum()
        self.discard_top3 += (topk == target_active.unsqueeze(1)).any(dim=1).sum()
        self.discard_count += target_active.numel()

    def update_claim(self, logits: torch.Tensor, batch: dict[str, torch.Tensor]) -> None:
        target = batch["claim_target"]
        active = target != IGNORE_INDEX
        if not torch.any(active):
            return
        masked = logits.masked_fill(~batch["claim_mask"].bool(), -1.0e4)
        pred = masked[active].argmax(dim=1)
        target_active = target[active].long()
        # [核心优化] GPU 级纯张量操作，消除 Python zip 带来的极度降速
        indices = target_active * 7 + pred 
        counts = torch.bincount(indices, minlength=49)
        self.claim_confusion += counts.view(7, 7)

    def update_hu(self, logits: torch.Tensor, batch: dict[str, torch.Tensor]) -> None:
        target = batch["hu_target"]
        active = target != IGNORE_INDEX
        if not torch.any(active):
            return
        pred = logits[active].argmax(dim=1)
        positives = target[active].long() == 1
        self.hu_tp += ((pred == 1) & positives).sum()
        self.hu_fn += ((pred != 1) & positives).sum()

    def update_kong(self, logits: torch.Tensor, batch: dict[str, torch.Tensor]) -> None:
        target = batch["self_kong_target"]
        active = target != IGNORE_INDEX
        if not torch.any(active):
            return
        pred = logits[active].argmax(dim=1)
        positives = target[active].long() != 0
        self.kong_tp += ((pred != 0) & positives).sum()
        self.kong_fn += ((pred == 0) & positives).sum()

    def as_metrics(self) -> dict[str, float]:
        # 只在计算最终结果时（也就是 Epoch 结束时），才统一下发 .item() 获取数据
        claim_f1 = macro_f1(self.claim_confusion)
        return {
            "loss": self.loss_sum.item() / max(1, self.batch_count),
            "value_loss": self.value_loss_sum.item() / max(1, self.batch_count),
            "discard_top1": self.discard_top1.item() / max(1, self.discard_count.item()),
            "discard_top3": self.discard_top3.item() / max(1, self.discard_count.item()),
            "claim_macro_f1": claim_f1,
            "hu_recall": self.hu_tp.item() / max(1, self.hu_tp.item() + self.hu_fn.item()),
            "kong_recall": self.kong_tp.item() / max(1, self.kong_tp.item() + self.kong_fn.item()),
        }

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

def empty_metrics() -> dict[str, float]:
    return {"loss": 0.0, "value_loss": 0.0, "discard_top1": 0.0, "discard_top3": 0.0, "claim_macro_f1": 0.0, "hu_recall": 0.0, "kong_recall": 0.0}

def move_batch(batch: dict[str, torch.Tensor], device: torch.device) -> dict[str, torch.Tensor]:
    return {key: value.to(device, non_blocking=True) for key, value in batch.items()}

def save_checkpoint(path: Path, model: torch.nn.Module, metadata: dict[str, Any], metrics: dict[str, float], epoch: int) -> None:
    torch.save({"model_state": model.state_dict(), "metadata": metadata, "metrics": metrics, "epoch": epoch, "model_config": {"tile_plane_count": TILE_PLANE_COUNT, "scalar_feature_count": SCALAR_FEATURE_COUNT}}, path)

if __name__ == "__main__":
    main()