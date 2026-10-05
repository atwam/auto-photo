# Wrap-up (October 2026)

State of the project after the first build-out: what it does, how it is put together, the decisions
behind it, and what is left to do.

## What it does

From any portrait (uploaded, dropped, or taken with the camera) it produces passport/ID photos:

- a **single digital JPEG** (600 dpi, 827×1063 px for 35×45 mm) for online applications, and
- a **print sheet**: 6 photos on 10×15 cm or 4×6 in (300 dpi JPEG) with cut marks and a "seal" panel
  listing the compliance checks.

Everything runs locally: a Rust CLI, and the same Rust core compiled to WebAssembly in the browser.
No photo is ever uploaded. The web UI is in English, French, German and Spanish.

Supported specs: **ICAO/Schengen**, **France**, **United Kingdom**, **Germany** (see caveats below).

## Architecture

```
crates/photo-core    the whole pipeline (pure Rust, no I/O; compiles natively and to wasm)
  detect.rs            YuNet face detection + MediaPipe 478-point face mesh
  segment.rs           MODNet person matte on a region of interest
  geometry.rs          head measurements (chin, eyes, skull top), crop placement per spec
  matting.rs           matte refinement (guided filter) + local background estimate
  whitebalance.rs      illuminant estimate on the original photo
  tone.rs              render frame, white balance, exposure, background compositing
  checks.rs            compliance checks → report (with message keys for localisation)
  layout.rs, draw.rs   print sheet, seal, text rendering (embedded Inter font)
  spec.rs              per-country specs as data
  pipeline.rs          locate → segment → produce
crates/photo-cli     `auto-photo` command (photo + sheet + report, --json)
crates/photo-wasm    wasm-bindgen API (Engine/Portrait/Rendered) used by the web worker
web/                 Vite + TypeScript, no framework
  src/main.ts          screens, processing animation sequence, result editor, downloads
  src/worker.ts        runs the wasm engine; downloads + caches models (Cache API)
  src/camera.ts        camera, oval guide, live hints
  src/stage.ts         canvas "lab" animation
  src/i18n/            dictionaries + runtime; i18n-plugin.ts pre-renders /fr/ /de/ /es/
  public/_headers      production headers (CSP etc.) for Cloudflare/Netlify
models/              ONNX models (not committed; scripts/fetch_models.sh)
scripts/             model download/verification, f16 weight conversion
doc/                 this documentation
```

### Pipeline

1. **Detect** the face (YuNet, 640×640 letterbox). Exactly one significant face is required.
2. **Landmarks**: MediaPipe face mesh on a rotated crop, refined twice from its own output.
3. **Matte**: MODNet on a 480×640 region around the head (1.4× the frame, so zooming out still works).
4. **Measure**: chin (mesh), eye line (irises), skull top (see below), roll.
5. **Place**: scale so chin→crown hits the spec's target, level the eyes, centre, keep the eye line in range.
6. **Render** the frame from the original photo (box-prefiltered resampling), refine the matte at output
   resolution, estimate the local original background.
7. **Tone**: white balance (illuminant from the whole original photo), highlight-based exposure, soft clip,
   background replacement by `out = I + (1−α)·b·(N − E)` (removes the old background's colour from hair).
8. **Check** and report; **lay out** the sheet.

The web worker runs steps 1–2 (`locate`) and 3–4 (`segment`) separately so the UI can animate real
progress; renders at 400 dpi for the live preview and re-renders on every slider change.

## Decisions and findings worth remembering

- **Inference with `tract`** (pure Rust, works in wasm). `ort` doesn't target wasm well.
- **Models** (all permissive): YuNet (MIT), MediaPipe face landmarker (Apache-2.0), MODNet (Apache-2.0).
  The quantized MODNet was slower in tract and left artefacts; fp16 compute produced NaNs. Solution:
  store weights as f16 with a `Cast` to f32 (`scripts/f16_weights.py`) → half the download, f32 compute.
  Total ~16 MB (target was 5–10 MB; accepted).
- **Head height is chin → skull, not hair**, for all four specs (ICAO: "top of the head if there were no
  hair"; gov.uk: "the top of the head, not the top of the hair"). The skull is estimated from the
  eye→upper-forehead distance (expression-independent, unlike eye→chin which grows when smiling), blended
  with the hair top from the matte minus a typical hair thickness, within anatomical bounds.
- **Germany no longer accepts printed photos** for passports/ID since May 2025 (transition ended
  31 July 2025). The spec remains (useful for other documents/visas) but the UI and sheet warn.
- **UK digital photos must be "unaltered by computer software"**: background replacement triggers a warning.
- **France measures without hair; white backgrounds are forbidden**; online applications need an ANTS
  e-photo code (certified booth/photographer), so the digital JPEG is mainly useful elsewhere.
- **White balance** uses three cues (eye whites, background median, gray-edge) with medoid outlier
  rejection. The first version only looked at "unsaturated" background pixels and therefore did nothing
  under tungsten light.
- **Hair edges**: the network matte is soft and upsampled (~10 px/mm vs 24 px/mm output). It's tightened,
  then a colour guided filter restores softness only along image edges. A naive guided filter created
  ghosts over busy backgrounds — the tightening step fixed that.
- **Checks** return message keys + numbers so the UI localises them; the Rust side keeps English text for
  the CLI. The sheet's seal text is passed in from the UI.
- **Camera**: switching uses an exact `facingMode`, falling back to cycling device ids; mirroring follows
  the camera actually opened.
- **Privacy guarantee is enforced** in production by CSP `connect-src 'self'` (see `web/public/_headers`).

## Status

Working and tested end-to-end in Chrome (desktop + mobile emulation + fake camera), and on a real phone
over the LAN. Rust unit tests: placement, sheet layout, guided filter.

| Area | State |
| --- | --- |
| CLI | done (`auto-photo`, `--json`, exit code 2 on failures) |
| Web: upload / drag-and-drop / camera with hints | done |
| Processing animation | done |
| Result editor (background, light, white balance, framing, guides, compare) | done |
| Downloads (digital JPEG with DPI, print sheet 10×15 / 4×6) | done |
| i18n (en/fr/de/es), legal footer | done — translations not reviewed by native speakers |
| Production build + headers | done, not deployed (see [deployment.md](deployment.md)) |

## Backlog

Roughly in order of value:

1. **Real-world tuning**: run a set of genuine phone selfies (plain walls, tungsten light, glasses,
   long/curly hair) through `cargo run --example wb_test` / the CLI; adjust skull estimate, white-balance
   priors and check thresholds.
2. **Offline/PWA**: service worker precaching HTML/JS/wasm (models are already cached) + web manifest.
3. **More checks**: glasses glare, shadows on the face, headwear, pitch (head tilted up/down).
4. **Smaller download**: `wasm-opt` (binaryen), trimming tract ops; smaller matting model if quality allows.
5. **More specs**: US 2×2 in, Canada, China visa, etc. (data only, see development.md).
6. **Pre-launch items** listed in [deployment.md](deployment.md#pre-launch-checklist).
7. Native-speaker review of FR/DE/ES, especially the legal text.

## Sources for the spec facts

- ICAO portrait guidelines: crown = "top of the head if there were no hair", chin→crown 70–80%.
- gov.uk passport photos and photographer guidance: 29–34 mm chin→crown (not hair), cream/light grey,
  digital ≥600×750 px, 50 KB–10 MB, no digital alteration.
- ANTS / service-public.fr: 32–36 mm chin→top of skull excluding hair, light uniform background, white forbidden.
- Bundesdruckerei Fotomustertafel and the 2025 digital-only rule (heise.de, municipal notices).
