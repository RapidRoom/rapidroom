"""Generate the tiny test-only graph; no learned weights or depth inference.

Run with ONNX's Python package. This graph averages the normalized RGB channels,
so production input letterboxing/cropping can be tested independently of a model.
"""
from pathlib import Path
import onnx
from onnx import TensorProto, helper

edge = 518
node = helper.make_node('ReduceMean', ['pixel_values'], ['predicted_depth'], axes=[1], keepdims=0)
graph = helper.make_graph([node], 'RapidRoom-depth-geometry-fixture',
    [helper.make_tensor_value_info('pixel_values', TensorProto.FLOAT, [1, 3, edge, edge])],
    [helper.make_tensor_value_info('predicted_depth', TensorProto.FLOAT, [1, edge, edge])])
model = helper.make_model(graph, producer_name='RapidRoom geometry test',
    opset_imports=[helper.make_opsetid('', 13)])
model.ir_version = 7
onnx.checker.check_model(model)
onnx.save(model, Path(__file__).with_name('channel-mean.onnx'))
