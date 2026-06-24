from __future__ import annotations

import json
from pathlib import Path

from run_sft_pipeline import EXPECTED_METADATA_SCHEMA_VERSION, dataset_status


def test_dataset_status_detects_complete_export(tmp_path: Path) -> None:
    (tmp_path / "metadata.json").write_text(
        json.dumps({"schema_version": EXPECTED_METADATA_SCHEMA_VERSION}),
        encoding="utf-8",
    )
    for name in ("train.jsonl", "val.jsonl", "test.jsonl"):
        (tmp_path / name).write_text("{}\n", encoding="utf-8")

    status = dataset_status(tmp_path)

    assert status.complete
    assert status.metadata_schema_version == EXPECTED_METADATA_SCHEMA_VERSION
    assert status.missing_files == ()


def test_dataset_status_reports_missing_split(tmp_path: Path) -> None:
    (tmp_path / "metadata.json").write_text(
        json.dumps({"schema_version": EXPECTED_METADATA_SCHEMA_VERSION}),
        encoding="utf-8",
    )
    (tmp_path / "train.jsonl").write_text("{}\n", encoding="utf-8")

    status = dataset_status(tmp_path)

    assert not status.complete
    assert status.missing_files == ("val.jsonl", "test.jsonl")
