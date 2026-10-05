# Development guide

## Prerequisites

| Tool | Version used | Notes |
| --- | --- | --- |
| Rust (rustup) | 1.98 (edition 2024 needs ≥ 1.85) | `rustup target add wasm32-unknown-unknown` |
| wasm-bindgen-cli | **0.2.129** | must match the `wasm-bindgen` version in `Cargo.lock`: `cargo install wasm-bindgen-cli --version 0.2.129 --locked` |
| Node.js | 26 (Vite 8 needs ≥ 20.19) | |
| Python 3 | any recent | used once by `scripts/fetch_models.sh` (creates `.venv` with `onnx`) |
| binaryen (`wasm-opt`) | optional | `brew install binaryen`; the wasm build uses it automatically if present |

## Everyday commands

```sh
./scripts/fetch_models.sh          # once: models/ (yunet, face_mesh, modnet .onnx)
cargo test --workspace --exclude photo-wasm
cargo run --release -p photo-cli -- photo.jpg -s fr -o /tmp/out

cd web
npm install                        # once
npm run wasm                       # after any Rust change (≈1.5 min): rebuilds src/pkg + copies models
npm run dev                        # http://localhost:5173, hot reload for TS/CSS
npm run dev:lan                    # https://<your-ip>:5173 for phones (self-signed cert)
npm run build && npm run preview   # production build in dist/, served at http://localhost:4173
```

`npm run wasm` is needed whenever anything under `crates/` changes; TypeScript/CSS changes hot-reload.

## Developer tools (Rust examples)

Run from the repo root with `cargo run --release --example <name> -p photo-core -- <photos…>`.

| Example | Purpose |
| --- | --- |
| `spike` | Writes `<photo>.debug.jpg`: landmarks, YuNet points, matte overlay, timings. |
| `calib` | Prints hair-top and forehead ratios used to calibrate the skull estimate (`geometry.rs`). |
| `matte_debug` | Writes the refined alpha matte of the ICAO frame as PNG (`OUT=dir`). |
| `wb_test r g b` | Applies a synthetic colour cast (linear gains) and prints what each white-balance cue estimates. `1 1 1` shows the cues on the original photo; `1.0 0.7 0.36` ≈ tungsten. |
| `probe` | Prints ONNX model inputs/outputs as tract sees them. |

Test photos: `testdata/*.jpg` is git-ignored. The public-domain portraits used so far came from Wikimedia
Commons (US presidential portraits, Einstein); add your own real selfies for tuning.

## Where to tune

| What | Where |
| --- | --- |
| Skull estimate | `geometry.rs`: `SKULL_FROM_FOREHEAD`, `HAIR_THICKNESS`, `SKULL_MIN/MAX` |
| Framing (space above head, eye-line margin) | `geometry.rs`: `TOP_SHARE`, `EYE_MARGIN_MM` |
| Check thresholds | `checks.rs` (each check is a few lines) |
| White balance | `whitebalance.rs`: cue weights, `SCLERA_TINT`, `OUTLIER`, `DEAD_ZONE`, `MAX_LOG` |
| Exposure target | `tone.rs` (`srgb_to_linear(0.86)` for skin highlights) |
| Hair edges | `matting.rs::refine_alpha` (tightening curve, radius, `eps`) |
| Camera hints | `web/src/camera.ts::evaluate` |

## Adding a spec

1. Add a `Spec` entry to `SPECS` in `crates/photo-core/src/spec.rs` (sizes in mm, head range, eye range,
   background colour, digital limits, `print_accepted`, notes, sources).
2. Add `spec.chip.<id>`, `spec.name.<id>`, `spec.notes.<id>` to **every** language in
   `web/src/i18n/messages.ts` (TypeScript errors if a language is missing a key).
3. `npm run wasm`. The CLI's `--spec` picks it up automatically.

Non-35×45 formats (e.g. US 2×2 in) work with the same pipeline; check that the sheet layout in
`layout.rs` still fits (it assumes a 3×2 grid plus a seal column).

## Adding a language

1. Add the code to `LANGS` and `LANG_NAMES` in `web/src/i18n/messages.ts` and a full dictionary (copy
   `en`, translate; the `Dict` type enforces completeness).
2. Add a link in the footer of `web/index.html` (`<a href="/xx/" data-lang="xx" …>`).
3. Build: the plugin emits `/xx/index.html` automatically.

The sheet font (Inter, Latin subset) covers Western European languages; other scripts need another
font in `crates/photo-core/assets/`.

## Tests

- `cargo test --workspace --exclude photo-wasm`: placement for every spec, sheet geometry for both papers,
  guided filter behaviour.
- `npx tsc --noEmit` in `web/`.
- End-to-end checks were done with Playwright + system Chrome (scripts kept outside the repo). A fake
  camera can be fed with `--use-file-for-fake-video-capture=file.y4m` (make one with
  `ffmpeg -loop 1 -i photo.jpg -vf scale=720:900,format=yuv420p -t 3 -r 10 cam.y4m`).
