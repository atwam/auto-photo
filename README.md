# auto·photo

Passport / ID photos from any portrait, processed entirely on-device. It finds the face, straightens and
crops it to the official size, fixes white balance, exposure and background, checks the result against
the rules, and produces:

- a single upload-ready JPEG (600 dpi by default, e.g. 827×1063 px for 35×45 mm), and
- a print sheet with 6 photos plus a check "seal" on 10×15 cm or 4×6 in paper (JPEG, 300 dpi).

The same Rust core runs as a CLI and, compiled to WebAssembly, in the browser. Nothing is uploaded.

## Layout

```
crates/photo-core   pipeline: detection, landmarks, matting, geometry, tone, checks, sheet
crates/photo-cli    `auto-photo` command
crates/photo-wasm   wasm-bindgen API used by the web app (runs in a Web Worker)
web/                Vite + TypeScript UI: upload or camera, animated processing, editor, downloads
models/             ONNX models (not committed; see below)
scripts/            model download + f16 weight conversion
```

## Models

`./scripts/fetch_models.sh` downloads, verifies and converts the three models (~16 MB total):

| Model | Use | License |
| --- | --- | --- |
| YuNet 2023mar (OpenCV Zoo) | face detection | MIT |
| MediaPipe Face Landmarker, 478 points | chin, irises, eyelids, lips | Apache-2.0 |
| MODNet | person matte (background, hair) | Apache-2.0 |

Large weights are stored as float16 with a Cast back to float32 (`scripts/f16_weights.py`), which halves
the download while keeping all compute in float32. Inference uses [tract](https://github.com/sonos/tract).

## CLI

```sh
./scripts/fetch_models.sh
cargo run --release -p photo-cli -- portrait.jpg --spec fr            # icao | fr | uk | de
cargo run --release -p photo-cli -- *.jpg -s uk --paper 4x6 -o out/ --json
```

Options: `--background 0..1` (replacement strength), `--brightness <stops>`, `--dpi`, `--sheet-dpi`.
The exit code is 2 when a photo fails a check.

## Web app

```sh
cd web
npm install
npm run wasm      # builds the wasm package and copies the models into public/
npm run dev       # http://localhost:5173
npm run dev:lan   # HTTPS on 0.0.0.0 (self-signed) for testing the camera from a phone
npm run build     # static site in web/dist
```

The camera needs HTTPS on phones (localhost is fine on desktop). After the first visit, models are
cached, so the app works offline.

## Specs

Specs are data in `crates/photo-core/src/spec.rs`. All four current specs measure head height from the
chin to the **crown** (top of the skull, not the hair), per ICAO, gov.uk and ANTS guidance:

| Spec | Head | Background | Notes |
| --- | --- | --- | --- |
| ICAO / Schengen | 32–36 mm | light, uniform | eye line 22.5–31.5 mm from bottom (ISO/IEC 19794-5) |
| France | 32–36 mm | light grey / light blue, white forbidden | online applications need an ANTS e-photo code |
| UK | 29–34 mm | cream / light grey | digital photos must be unaltered; ≥600×750 px, 50 KB–10 MB |
| Germany | 32–36 mm | neutral light grey | printed photos no longer accepted for passports/ID since 2025 |

## Image processing notes

- **Matte refinement.** The network matte is computed at ~10 px/mm and is soft. At output resolution it is
  tightened, then a colour guided filter restores softness only along real image edges (hair strands).
  Compositing replaces the *local* original background colour, so old background colour doesn't bleed into hair.
- **White balance** is estimated on the whole original photo from three cues voting in log-chromaticity:
  eye whites (sclera, with a small warm prior), the background (median), and gray-edge. Cues far from the
  consensus are down-weighted, so a coloured wall doesn't skew the result. The UI slider is a manual
  cool↔warm offset around the automatic setting.
- **Camera hints** run the face detector on ~6 frames/s and guide distance, position, tilt, yaw and lighting.

## Known limitations

- The skull top is estimated from face proportions blended with the hair matte. It is usually within
  the ±2 mm tolerance but can be off with very voluminous hair or unusual head shapes. Use the head-size
  slider and the guides overlay to check.
- Glasses glare, headwear and shadows on the face are not detected yet.
- Background replacement is a digital alteration; some authorities (UK) forbid it. The checker warns.
- The seal is an automated self-check, not an official certification.
