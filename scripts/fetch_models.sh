#!/usr/bin/env bash
# Downloads the ONNX models, verifies checksums, and stores large ones with f16
# weights (f32 compute) to halve their size. Output: models/{yunet,face_mesh,modnet}.onnx
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p models/src

fetch() { # url file sha256
  if [ ! -f "models/src/$2" ]; then curl -fsSL -o "models/src/$2" "$1"; fi
  echo "$3  models/src/$2" | shasum -a 256 -c --quiet
}

# YuNet face detector (MIT) - OpenCV Zoo
fetch https://huggingface.co/opencv/face_detection_yunet/resolve/main/face_detection_yunet_2023mar.onnx \
  yunet.onnx 8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4
# MediaPipe face landmarker, 478 points (Apache-2.0)
fetch https://huggingface.co/senty-au/face_landmarks_detector-ONNX/resolve/main/onnx/model.onnx \
  face_mesh.onnx 7d6e82dee82a1dca5fbddb282b3cc74571833a530de317fc22ae325c3358beeb
# MODNet portrait matting (Apache-2.0)
fetch https://huggingface.co/Xenova/modnet/resolve/main/onnx/model.onnx \
  modnet.onnx 07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9

if [ ! -x .venv/bin/python ]; then python3 -m venv .venv && .venv/bin/pip install -q onnx numpy; fi
cp models/src/yunet.onnx models/yunet.onnx
.venv/bin/python scripts/f16_weights.py models/src/face_mesh.onnx models/face_mesh.onnx
.venv/bin/python scripts/f16_weights.py models/src/modnet.onnx models/modnet.onnx
ls -l models/*.onnx
