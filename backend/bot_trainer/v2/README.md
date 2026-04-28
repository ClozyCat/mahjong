# Mahjong Bot Trainer V2

V2 uses Rust to parse BotZone match records and export backend-native decision samples. Python trains a multi-head model and exports the production ONNX file used by the Rust bot.

## Export Data

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -ProgressEvery 100
```

```bash
./backend/bot_trainer/v2/export_full_dataset.sh --progress-every 100
```

Small export:

```powershell
.\backend\bot_trainer\v2\export_full_dataset.ps1 -OutputDir backend/bot_trainer/v2/out_smoke -MaxMatches 100 -ProgressEvery 10
```

```bash
./backend/bot_trainer/v2/export_full_dataset.sh --output backend/bot_trainer/v2/out_smoke --max-matches 100 --progress-every 10
```

Default dataset path is `backend/bot_trainer/datasets/data.txt`.

## GPU Training

The training wrapper defaults to `uv run python`, automatic device selection, AMP, and batch size `4096`.

```powershell
.\backend\bot_trainer\v2\train_and_export_model.ps1 -Epochs 20 -BatchSize 4096 -Device cuda -NumWorkers 0
```

```bash
./backend/bot_trainer/v2/train_and_export_model.sh --epochs 20 --batch-size 4096 --device cuda --num-workers 0
```

If VRAM is tight, reduce the batch size to `1024`. If you want CPU training, use `-Device cpu -NoAmp` on Windows or `--device cpu --no-amp` on Linux.

The wrapper writes:

- `backend/bot_trainer/v2/checkpoints/best.pt`
- `backend/bot_trainer/v2/checkpoints/metrics.json`
- `backend/assets/models/mahjong_policy_net.onnx`

After ONNX export, it runs the Rust ONNX smoke test.

## Direct Commands

```powershell
uv run python backend/bot_trainer/v2/train.py --data backend/bot_trainer/v2/out --epochs 20 --batch-size 2048 --output backend/bot_trainer/v2/checkpoints --device cuda --amp
uv run python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --output backend/assets/models/mahjong_policy_net.onnx
```

## Model Outputs

- `discard_logits`: 34 tile logits.
- `claim_logits`: 7 claim logits: pass, hu, pung, kong, chow_left, chow_mid, chow_right.
- `self_kong_logits`: 3 logits: pass, concealed_kong, add_kong.
- `hu_logits`: 2 logits.
- `value`: expected score delta.
- `risk_logits`: 34 tile risk logits.
