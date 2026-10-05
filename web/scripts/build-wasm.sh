#!/usr/bin/env bash
# Builds the wasm module into web/src/pkg and copies the models into web/public.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
[ -f models/modnet.onnx ] || ./scripts/fetch_models.sh
cargo build -p photo-wasm --target wasm32-unknown-unknown --profile wasm
wasm-bindgen --target web --out-dir web/src/pkg target/wasm32-unknown-unknown/wasm/photo_wasm.wasm
if command -v wasm-opt >/dev/null; then
  wasm-opt -O3 --enable-simd --enable-bulk-memory --enable-nontrapping-float-to-int \
    web/src/pkg/photo_wasm_bg.wasm -o web/src/pkg/photo_wasm_bg.wasm
fi
mkdir -p web/public/models
cp models/yunet.onnx models/face_mesh.onnx models/modnet.onnx web/public/models/
