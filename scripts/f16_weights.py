"""Store an ONNX model's float32 weights as float16 while keeping all compute in
float32: each large initializer becomes f16 followed by a Cast back to f32,
which inference engines constant-fold at load time. Halves download size."""
import sys
import numpy as np
import onnx
from onnx import helper, numpy_helper, TensorProto

src, dst = sys.argv[1], sys.argv[2]
m = onnx.load(src)
g = m.graph
new_inits, casts = [], []
for init in g.initializer:
    arr = numpy_helper.to_array(init)
    if arr.dtype == np.float32 and arr.size >= 64:
        half = numpy_helper.from_array(arr.astype(np.float16), init.name + "__f16")
        new_inits.append(half)
        casts.append(helper.make_node("Cast", [half.name], [init.name], to=TensorProto.FLOAT))
    else:
        new_inits.append(init)
del g.initializer[:]
g.initializer.extend(new_inits)
nodes = casts + list(g.node)
del g.node[:]
g.node.extend(nodes)
onnx.checker.check_model(m)
onnx.save(m, dst)
