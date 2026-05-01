# Discard Sequence GRU Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional discard-order sequence input and a GRU encoder for the se_d4_film model family while keeping old two-input ONNX models as a runtime fallback.

**Architecture:** Rust stores and exports a chronological `discard_history` of `{seat_index, tile_key}` events. Python encodes that history as a fixed `64 x 38` float tensor: 34 one-hot tile features plus 4 relative-seat one-hot features. Models with `use_discard_sequence=True` concatenate a GRU sequence embedding into the trunk; models without it keep the existing `tile_planes + scalar_features` contract. ONNX export emits the extra `discard_sequence` input only when the checkpoint config requires it, and Rust runtime detects model inputs before binding the optional tensor.

**Tech Stack:** Rust backend/state/projection/features/ONNX runtime, Python trainer/dataset/model/export, existing arena/candidate gate scripts.

---

## File Map

- Modify: `backend/src/core/state/round.rs`
  - Add `DiscardEventState` and `RoundState.discard_history`.
- Modify: `backend/src/core/engine/planner.rs`
  - Initialize empty discard history at round start.
- Modify: `backend/src/rules/standard/actions.rs`
  - Append discard events whenever a discard is applied.
- Modify: `backend/src/room_scoring.rs`
  - Carry discard history into scoring cache.
- Modify: `backend/src/projection/bot_view.rs`
  - Add discard history to bot context.
- Modify: `backend/src/bot_trainer/replay.rs`
  - Export chronological discard history in training samples.
- Modify: `backend/src/bot/features.rs`
  - Encode discard history as `64 x 38` float features.
- Modify: `backend/src/bot/neural.rs`
  - Bind `discard_sequence` only when the loaded ONNX model declares that input.
- Modify: `backend/src/bot/arena.rs`
  - Include discard sequence in trajectory rows.
- Modify: `backend/bot_trainer/v2/dataset.py`
  - Add disk-cached `discard_sequence` tensor with fallback zeros for old rows.
- Modify: `backend/bot_trainer/v2/rl_dataset.py`
  - Add trajectory `discard_sequence` tensor fallback.
- Modify: `backend/bot_trainer/v2/model.py`
  - Add optional GRU sequence encoder and trunk-width adjustment.
- Modify: `backend/bot_trainer/v2/train.py`
  - Add `--use-discard-sequence`.
- Modify: `backend/bot_trainer/v2/rl_train.py`
  - Pass sequence tensor when config requires it.
- Modify: `backend/bot_trainer/v2/export_onnx.py`
  - Export two-input or three-input ONNX graph based on checkpoint config.
- Modify: `backend/bot_trainer/v2/train_and_export_model.ps1`
  - Expose `-UseDiscardSequence`.
- Modify: `backend/bot_trainer/v2/train_and_export_model.sh`
  - Expose `--use-discard-sequence`.
- Modify: `backend/bot_trainer/v2/README.md`
  - Document se_d4_film + GRU experiment and arena gate.

## Implementation Tasks

- [x] Add failing Python tests for dataset sequence encoding and GRU model output shapes.
- [x] Implement Python dataset/model/train/export sequence path.
- [x] Add failing Rust tests for replay/runtime discard history and feature shape.
- [x] Implement Rust state/context/features/neural sequence path.
- [x] Update wrappers and docs with the sequence experiment command.
- [x] Run Python and Rust targeted verification.

## Acceptance Gate

Only promote a sequence model if the rotated arena matrix beats the current se_d4_film production candidate on:

- average score delta,
- win rate non-regression,
- deal-in rate not worse by more than 2 percentage points,
- first-tenpai turn or final-tenpai rate improvement,
- average decision latency under 100 ms.
