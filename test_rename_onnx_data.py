from __future__ import annotations

import onnx
from onnx import TensorProto, helper

from rename_onnx_data import process_model, rewrite_external_data_locations


def external_initializer(name: str, location: str) -> TensorProto:
    tensor = helper.make_tensor(name, TensorProto.FLOAT, [1], [0.0])
    tensor.ClearField("raw_data")
    tensor.data_location = TensorProto.EXTERNAL
    tensor.external_data.add(key="location", value=location)
    tensor.external_data.add(key="offset", value="0")
    tensor.external_data.add(key="length", value="4")
    return tensor


def test_rewrite_external_data_locations_updates_all_initializers() -> None:
    graph = helper.make_graph(
        nodes=[],
        name="external-data-test",
        inputs=[],
        outputs=[],
        initializer=[
            external_initializer("first", "epoch_003.onnx.data"),
            external_initializer("second", "epoch_003.onnx.data"),
            external_initializer("third", "old_weights.data"),
        ],
    )
    model = helper.make_model(graph)

    old_locations = rewrite_external_data_locations(model, "weights.data")

    assert old_locations == {"epoch_003.onnx.data", "old_weights.data"}
    locations = [
        entry.value
        for tensor in model.graph.initializer
        for entry in tensor.external_data
        if entry.key == "location"
    ]
    assert locations == ["weights.data", "weights.data", "weights.data"]


def test_process_model_apply_saves_rewritten_model_and_copies_data(tmp_path) -> None:
    onnx_path = tmp_path / "policy.onnx"
    source_data = tmp_path / "epoch_003.onnx.data"
    source_data.write_bytes(b"1234")
    graph = helper.make_graph(
        nodes=[],
        name="external-data-test",
        inputs=[],
        outputs=[],
        initializer=[
            external_initializer("first", source_data.name),
            external_initializer("second", source_data.name),
        ],
    )
    onnx.save(helper.make_model(graph), onnx_path)

    changed = process_model(onnx_path, "weights.data", apply=True)

    assert changed
    assert (tmp_path / "weights.data").read_bytes() == b"1234"
    model = onnx.load(onnx_path, load_external_data=False)
    locations = [
        entry.value
        for tensor in model.graph.initializer
        for entry in tensor.external_data
        if entry.key == "location"
    ]
    assert locations == ["weights.data", "weights.data"]
