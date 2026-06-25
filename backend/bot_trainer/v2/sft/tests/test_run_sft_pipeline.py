from __future__ import annotations

import json
from pathlib import Path

from run_sft_pipeline import (
    EXPECTED_METADATA_SCHEMA_VERSION,
    dataset_status,
    merge_exported_datasets,
)


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


def test_merge_exported_datasets_concatenates_splits_and_keeps_metadata(tmp_path: Path) -> None:
    first = write_export_dir(tmp_path / "first", ["a"], ["b"], ["c"])
    second = write_export_dir(tmp_path / "second", ["d", "e"], [], ["f"])
    output = tmp_path / "merged"

    merge_exported_datasets((first, second), output)

    assert (output / "metadata.json").read_text(encoding="utf-8") == (
        first / "metadata.json"
    ).read_text(encoding="utf-8")
    assert (output / "train.jsonl").read_text(encoding="utf-8").splitlines() == [
        "a",
        "d",
        "e",
    ]
    assert (output / "val.jsonl").read_text(encoding="utf-8").splitlines() == ["b"]
    assert (output / "test.jsonl").read_text(encoding="utf-8").splitlines() == [
        "c",
        "f",
    ]


def write_export_dir(
    path: Path,
    train_rows: list[str],
    val_rows: list[str],
    test_rows: list[str],
) -> Path:
    path.mkdir(parents=True)
    (path / "metadata.json").write_text(
        json.dumps({"schema_version": EXPECTED_METADATA_SCHEMA_VERSION, "tile_keys": []}),
        encoding="utf-8",
    )
    for name, rows in {
        "train.jsonl": train_rows,
        "val.jsonl": val_rows,
        "test.jsonl": test_rows,
    }.items():
        (path / name).write_text(
            "".join(f"{row}\n" for row in rows),
            encoding="utf-8",
        )
    return path
