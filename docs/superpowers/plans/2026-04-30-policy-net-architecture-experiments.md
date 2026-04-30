# Policy Net Architecture Experiments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add low-risk model architecture experiment switches for SE blocks, deeper suit-aware ResNet encoders, and FiLM scalar modulation while preserving the production ONNX input/output contract.

**Architecture:** Keep Rust feature encoding and ONNX runtime unchanged: `tile_planes` remains `batch x 10 x 34`, `scalar_features` remains `batch x 10`, and all six output heads keep their names and shapes. Python model construction becomes configurable through `ModelConfig` and training CLI flags, with defaults matching the current production architecture.

**Tech Stack:** Python trainer (`torch`, `pytest`), existing V2 dataset schema, ONNX exporter, PowerShell/Bash training wrappers.

---

## Scope

This plan implements the first experiment round only:

- SE channel attention inside residual Conv1d blocks.
- Configurable suited/honor residual block counts.
- Optional FiLM modulation from raw scalar features into the final tile embedding.
- Checkpoint `model_config` persistence so export and RL warm starts instantiate the same architecture.

Deferred:

- Discard-order GRU/LSTM sequence input. That requires Rust context/schema changes, dataset cache versioning, ONNX input changes, and runtime input binding.

## File Map

- Modify: `backend/bot_trainer/v2/model.py`
  - Add `SEBlock`, configurable residual depth, optional FiLM, and richer `ModelConfig`.
- Modify: `backend/bot_trainer/v2/test_model.py`
  - Add red/green tests for architecture defaults, parameter growth, FiLM zero-init, and config serialization.
- Modify: `backend/bot_trainer/v2/train.py`
  - Add CLI flags and save the full architecture config in checkpoints.
- Modify: `backend/bot_trainer/v2/rl_train.py`
  - Instantiate model architecture from checkpoint config for PPO warm starts.
- Modify: `backend/bot_trainer/v2/export_onnx.py`
  - Load full architecture config from checkpoint before export.
- Modify: `backend/bot_trainer/v2/train_and_export_model.ps1`
  - Expose architecture flags.
- Modify: `backend/bot_trainer/v2/train_and_export_model.sh`
  - Expose architecture flags.
- Modify: `backend/bot_trainer/v2/README.md`
  - Document recommended experiment matrix and acceptance gates.

## Task 1: Tests First

- [x] Add model tests proving default config keeps output shapes.
- [x] Add model tests proving SE/depth/FiLM variants keep output shapes and increase parameter count.
- [x] Add a test proving FiLM starts as identity modulation through zero-initialized final layer.
- [x] Add a test proving `ModelConfig.from_dict` fills defaults for old checkpoints.
- [x] Run `python -m pytest backend/bot_trainer/v2/test_model.py -q` and confirm the new tests fail before implementation.

## Task 2: Implement Model Architecture Switches

- [x] Extend `ModelConfig` with `suited_block_count`, `honor_block_count`, `use_se`, `se_reduction`, and `film_scalar`.
- [x] Add `SEBlock` and thread it through `ResidualConvBlock`.
- [x] Replace hard-coded encoder blocks with configurable block factories.
- [x] Add raw-scalar FiLM over the 512-d tile embedding, zero-initialized so it starts as identity.
- [x] Keep `build_model(ModelConfig(10, 10))` behavior equivalent to the current default architecture.
- [x] Run `python -m pytest backend/bot_trainer/v2/test_model.py -q`.

## Task 3: Preserve Config Through Training, RL, And Export

- [x] Add supervised training CLI flags:
  - `--suited-block-count`
  - `--honor-block-count`
  - `--use-se`
  - `--se-reduction`
  - `--film-scalar`
- [x] Save the full `ModelConfig` into checkpoint `model_config`.
- [x] Make `export_onnx.py` instantiate from full checkpoint config with old-checkpoint defaults.
- [x] Make `rl_train.py` instantiate from checkpoint config when a checkpoint is present.
- [x] Run `python -m pytest backend/bot_trainer/v2/test_model.py -q`.

## Task 4: Wrapper Scripts And Docs

- [x] Add matching architecture parameters to PowerShell wrapper.
- [x] Add matching architecture options to Bash wrapper.
- [x] Document experiment matrix:
  - current default
  - `--use-se`
  - `--use-se --suited-block-count 4 --honor-block-count 2`
  - `--use-se --suited-block-count 6 --honor-block-count 3`
  - `--use-se --suited-block-count 4 --honor-block-count 2 --film-scalar`
- [x] Document that discard-order GRU is deferred until schema/runtime upgrade.

## Final Verification

- [x] Run `python -m pytest backend/bot_trainer/v2/test_model.py -q`.
- [x] Run `python backend/bot_trainer/v2/export_onnx.py --checkpoint backend/bot_trainer/v2/checkpoints/best.pt --output backend/bot_trainer/v2/checkpoints/architecture_smoke.onnx` if a local checkpoint exists.
- [x] Local checkpoint existed, so ONNX export smoke was run.
