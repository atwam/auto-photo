import { Camera } from "./camera";
import type { Adjust, Face, RenderInfo, RenderOpts, Req, Res, Spec, Status, Tone } from "./protocol";
import { Stage, ease, lerpView, scanWhile, tween } from "./stage";

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const MAX_SIDE = 2400;
const MODELS_BYTES = 15_750_000; // approximate, for the progress bar
const PREVIEW_DPI = 400;
const PHOTO_DPI = 600;
const SPEC_LABELS: Record<string, string> = { icao: "EU / ICAO", fr: "France", uk: "United Kingdom", de: "Germany" };
const NO_TONE: Tone = { white_balance: 0, auto_exposure: 0, brightness_ev: 0, background: 0, background_color: null };
const DEFAULT_TONE: Tone = { white_balance: 1, auto_exposure: 1, brightness_ev: 0, background: 1, background_color: null };
const DEFAULT_ADJUST: Adjust = { dx_mm: 0, dy_mm: 0, zoom: 1, rotate_deg: 0 };
const BG_SWATCHES: { name: string; rgb: [number, number, number]; white?: boolean }[] = [
  { name: "Light grey", rgb: [224, 224, 222] },
  { name: "Cool grey", rgb: [218, 222, 226] },
  { name: "Cream", rgb: [234, 228, 214] },
  { name: "Light blue", rgb: [212, 224, 236] },
  { name: "Off-white", rgb: [242, 242, 240], white: true },
];

// ---------------------------------------------------------------- worker ---

const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
type Pending = { resolve: (r: Res) => void; reject: (e: Error) => void };
const waiting = new Map<string, Pending>();
let nextId = 1;

worker.onmessage = (e: MessageEvent<Res>) => {
  const msg = e.data;
  if (msg.type === "progress") return onModelProgress(msg.loaded);
  const key =
    msg.type === "error"
      ? (msg.id !== undefined ? `id:${msg.id}` : `step:${msg.step}`)
      : "id" in msg
        ? `id:${msg.id}`
        : `step:${({ ready: "init", located: "locate", segmented: "segment" } as Record<string, string>)[msg.type]}`;
  const p = waiting.get(key);
  if (!p) return;
  waiting.delete(key);
  if (msg.type === "error") p.reject(new Error(msg.message));
  else p.resolve(msg);
};

function call<T extends Res["type"]>(req: Req, transfer: Transferable[] = []): Promise<Extract<Res, { type: T }>> {
  const key = "id" in req ? `id:${req.id}` : `step:${req.type}`;
  return new Promise((resolve, reject) => {
    waiting.set(key, { resolve: resolve as (r: Res) => void, reject });
    worker.postMessage(req, transfer);
  });
}

// ----------------------------------------------------------------- state ---

const state = {
  specs: [] as Spec[],
  spec: localStorage.getItem("spec") ?? "fr",
  paper: "10x15" as "10x15" | "4x6",
  tone: { ...DEFAULT_TONE },
  adjust: { ...DEFAULT_ADJUST },
  source: null as HTMLCanvasElement | null,
  face: null as Face | null,
  info: null as RenderInfo | null,
  original: null as HTMLCanvasElement | null,
  rendered: null as HTMLCanvasElement | null,
  busy: false,
};

function currentSpec(): Spec {
  return state.specs.find((s) => s.id === state.spec) ?? state.specs[0];
}

const ready = call<"ready">({ type: "init" }).then((r) => {
  state.specs = r.specs;
  if (!state.specs.some((s) => s.id === state.spec)) state.spec = "icao";
  renderSpecPicker($("spec-picker"));
  const ms = $("model-status");
  ms.classList.add("ready");
  ms.querySelector(".txt")!.textContent = "Ready — works offline from now on";
});
ready.catch((e) => {
  $("model-status").querySelector(".txt")!.textContent = `Could not load models: ${e.message}`;
});

function onModelProgress(loaded: number) {
  const pct = Math.min(100, (loaded / MODELS_BYTES) * 100);
  ($("model-status").querySelector(".bar span") as HTMLElement).style.width = `${pct}%`;
  $("model-status").querySelector(".txt")!.textContent = `Downloading face models… ${(loaded / 1e6).toFixed(1)} MB`;
}

// --------------------------------------------------------------- helpers ---

function show(screen: "start" | "camera" | "process" | "result") {
  document.querySelectorAll(".screen").forEach((s) => s.classList.remove("active"));
  $(`screen-${screen}`).classList.add("active");
  window.scrollTo({ top: 0 });
}

let toastTimer = 0;
function toast(text: string, error = false) {
  const t = $("toast");
  t.textContent = text;
  t.classList.toggle("error", error);
  t.classList.add("show");
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => t.classList.remove("show"), 4200);
}

function fitCanvas(src: CanvasImageSource & { width: number; height: number }): HTMLCanvasElement {
  const s = Math.min(1, MAX_SIDE / Math.max(src.width, src.height));
  const c = document.createElement("canvas");
  c.width = Math.round(src.width * s);
  c.height = Math.round(src.height * s);
  const ctx = c.getContext("2d")!;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(src, 0, 0, c.width, c.height);
  return c;
}

function rgbaCanvas(w: number, h: number, rgba: Uint8Array): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  c.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer as ArrayBuffer, rgba.byteOffset, rgba.length), w, h), 0, 0);
  return c;
}

async function render(opts: RenderOpts) {
  const r = await call<"rendered">({ type: "render", id: nextId++, spec: state.spec, opts });
  return { canvas: rgbaCanvas(r.width, r.height, r.rgba), info: r.info };
}

function renderSpecPicker(el: HTMLElement, onChange?: () => void) {
  el.innerHTML = "";
  for (const s of state.specs) {
    const b = document.createElement("button");
    b.className = "chip" + (s.id === state.spec ? " on" : "");
    b.textContent = SPEC_LABELS[s.id] ?? s.name;
    b.title = s.name;
    b.setAttribute("role", "radio");
    b.setAttribute("aria-checked", String(s.id === state.spec));
    b.onclick = () => {
      state.spec = s.id;
      localStorage.setItem("spec", s.id);
      el.querySelectorAll(".chip").forEach((c) => c.classList.toggle("on", c === b));
      onChange?.();
    };
    el.appendChild(b);
  }
}

/** Background tinted and rasterised with scan lines; the person stays clear. */
function matteOverlay(alpha: Uint8Array, w: number, h: number): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const img = new ImageData(w, h);
  for (let y = 0; y < h; y++) {
    const line = y % 3 === 0 ? 1 : 0.45;
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      const bg = 1 - alpha[i] / 255;
      // Highlight the matte edge.
      const edge = alpha[i] > 20 && alpha[i] < 235 ? 1 : 0;
      img.data[i * 4] = edge ? 200 : 20;
      img.data[i * 4 + 1] = edge ? 255 : 150;
      img.data[i * 4 + 2] = edge ? 243 : 140;
      img.data[i * 4 + 3] = Math.round(255 * Math.max(edge * 0.9, bg * 0.62 * line));
    }
  }
  c.getContext("2d")!.putImageData(img, 0, 0);
  return c;
}

// ------------------------------------------------------------ processing ---

const STEPS = [
  ["face", "Finding your face"],
  ["mesh", "Mapping 478 landmarks"],
  ["matte", "Separating you from the background"],
  ["measure", "Measuring crown, chin & eye line"],
  ["crop", "Straightening & cropping"],
  ["tone", "Balancing white & exposure"],
  ["bg", "Cleaning the background"],
  ["check", "Checking the rules"],
] as const;
type StepId = (typeof STEPS)[number][0];

function stepUi() {
  const ol = $("steps");
  ol.innerHTML = STEPS.map(([id, label]) => `<li data-step="${id}"><span class="ico"></span><span>${label}<small></small></span></li>`).join("");
  const li = (id: StepId) => ol.querySelector(`[data-step="${id}"]`) as HTMLElement;
  return {
    start: (id: StepId) => li(id).classList.add("active"),
    done: (id: StepId, note = "") => {
      li(id).classList.replace("active", "done");
      li(id).querySelector("small")!.textContent = note;
    },
    fail: (id: StepId, note: string) => {
      li(id).classList.remove("active");
      li(id).classList.add("failed");
      li(id).querySelector("small")!.textContent = note;
    },
  };
}

let stage: Stage | null = null;

async function processPhoto(input: HTMLCanvasElement) {
  if (state.busy) return;
  state.busy = true;
  state.adjust = { ...DEFAULT_ADJUST };
  const src = fitCanvas(input);
  state.source = src;
  show("process");
  const steps = stepUi();
  stage?.destroy();
  stage = new Stage($<HTMLCanvasElement>("stage"));
  stage.source = src;
  stage.view = stage.fitView();
  const caption = $("stage-caption");
  const spec = () => currentSpec();

  try {
    caption.textContent = "LOADING MODELS";
    await scanWhile(stage, "", ready, 300);

    // 1. Face.
    steps.start("face");
    caption.textContent = `INPUT ${src.width}×${src.height}`;
    const bitmap = await createImageBitmap(src);
    const located = await scanWhile(stage, "DETECTING FACE", call<"located">({ type: "locate", bitmap }, [bitmap]));
    stage.face = located.face;
    await tween(550, (t) => (stage!.bracket = t));
    steps.done("face");

    // 2. Landmarks.
    steps.start("mesh");
    await tween(1000, (t) => (stage!.meshShown = t));
    const rollDeg = (located.face.head.roll * 180) / Math.PI;
    steps.done("mesh", `head tilt ${rollDeg.toFixed(1)}°`);

    // 3. Matte.
    steps.start("matte");
    const ms = Math.min(1, 900 / Math.max(src.width, src.height));
    const mw = Math.round(src.width * ms), mh = Math.round(src.height * ms);
    const seg = await scanWhile(stage, "SEGMENTING", call<"segmented">({ type: "segment", width: mw, height: mh }));
    stage.face = seg.face;
    state.face = seg.face;
    stage.matte = matteOverlay(seg.matte, mw, mh);
    await tween(900, (t) => (stage!.matteT = ease(t)));
    steps.done("matte");

    // 4. Measures.
    steps.start("measure");
    await tween(900, (t) => (stage!.measureT = t));
    steps.done("measure");

    // 5. Crop: fly into the frame.
    steps.start("crop");
    const rawP = render({ tone: NO_TONE, dpi: PREVIEW_DPI });
    const toneP = render({ tone: { ...state.tone, background: 0 }, dpi: PREVIEW_DPI });
    const fullP = render({ tone: state.tone, dpi: PREVIEW_DPI });
    const raw = await rawP;
    const { w, h } = stage.size;
    const s = spec();
    const fh = h * 0.84, fw = (fh * s.width_mm) / s.height_mm;
    const rect = { x: (w - Math.min(fw, w * 0.9)) / 2, y: (h - fh) / 2, w: Math.min(fw, w * 0.9), h: Math.min(fw, w * 0.9) * (s.height_mm / s.width_mm) };
    rect.y = (h - rect.h) / 2;
    const v0 = stage.view;
    const v1 = stage.cropView(raw.info.transform, s.width_mm, rect);
    stage.frameRect = rect;
    const pivot = seg.face.head.eyes;
    const matteT0 = stage.matteT;
    await tween(1400, (t) => {
      const e = ease(t);
      stage!.view = lerpView(v0, v1, e, pivot);
      stage!.dim = e;
      stage!.matteT = matteT0 * (1 - e);
    });
    stage.face = null;
    stage.matte = null;
    caption.textContent = `${s.width_mm}×${s.height_mm} MM · HEAD ${raw.info.placement.head_mm.toFixed(1)} MM`;
    steps.done("crop", `${(-rollDeg).toFixed(1)}° rotation, ${s.width_mm}×${s.height_mm} mm`);

    // 6. Tone pass.
    steps.start("tone");
    const toned = await toneP;
    const ev = toned.info.tone.exposure_ev;
    const layer1 = { img: toned.canvas, t: 0, label: `EXPOSURE ${ev >= 0 ? "+" : ""}${ev.toFixed(2)} EV` };
    stage.layers.push(layer1);
    await tween(1200, (t) => (layer1.t = ease(t)));
    steps.done("tone", `${ev >= 0 ? "+" : ""}${ev.toFixed(2)} EV, white balance corrected`);

    // 7. Background pass.
    steps.start("bg");
    const full = await fullP;
    const layer2 = { img: full.canvas, t: 0, label: "BACKGROUND REPLACED" };
    stage.layers.push(layer2);
    await tween(1200, (t) => (layer2.t = ease(t)));
    layer1.label = "";
    steps.done("bg");

    // 8. Checks.
    steps.start("check");
    layer2.label = "";
    await tween(900, (t) => (stage!.scan = t));
    stage.scan = null;
    const rep = full.info.report;
    const fails = rep.checks.filter((c) => c.status === "fail").length;
    const warns = rep.checks.filter((c) => c.status === "warn").length;
    steps.done("check", fails ? `${fails} problem${fails > 1 ? "s" : ""}` : warns ? `${warns} warning${warns > 1 ? "s" : ""}` : "all passed");

    state.original = raw.canvas;
    state.rendered = full.canvas;
    state.info = full.info;
    await new Promise((r) => setTimeout(r, 500));
    openResult();
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    const active = document.querySelector("#steps li.active") as HTMLElement | null;
    if (active) steps.fail(active.dataset.step as StepId, msg);
    stage.scan = null;
    toast(msg, true);
    setTimeout(() => show("start"), 3500);
  } finally {
    state.busy = false;
  }
}

// ---------------------------------------------------------------- result ---

function openResult() {
  show("result");
  renderSpecPickerInResult();
  syncControls();
  paintResult();
}

function renderSpecPickerInResult() {
  let el = document.getElementById("result-spec");
  if (!el) {
    el = document.createElement("div");
    el.id = "result-spec";
    el.className = "chips small";
    el.style.marginBottom = "14px";
    $("screen-result").querySelector(".result-side")!.prepend(el);
  }
  renderSpecPicker(el, () => {
    renderSwatches();
    void rerender();
  });
  renderSwatches();
}

function renderSwatches() {
  const spec = currentSpec();
  const el = $("bg-swatches");
  el.innerHTML = "";
  const options = [{ name: "Spec default", rgb: spec.background }, ...BG_SWATCHES.filter((s) => !(s.white && spec.white_background_forbidden))];
  for (const o of options) {
    const b = document.createElement("button");
    b.className = "swatch";
    b.title = o.name;
    b.style.background = `rgb(${o.rgb.join(",")})`;
    const isOn = o.name === "Spec default" ? state.tone.background_color === null : state.tone.background_color?.join() === o.rgb.join();
    b.classList.toggle("on", isOn);
    b.onclick = () => {
      state.tone.background_color = o.name === "Spec default" ? null : o.rgb;
      renderSwatches();
      void rerender();
    };
    el.appendChild(b);
  }
}

const SLIDERS: [string, (v: number) => void, () => number][] = [
  ["s-bg", (v) => (state.tone.background = v), () => state.tone.background],
  ["s-bright", (v) => (state.tone.brightness_ev = v), () => state.tone.brightness_ev],
  ["s-wb", (v) => (state.tone.white_balance = v), () => state.tone.white_balance],
  ["s-exp", (v) => (state.tone.auto_exposure = v), () => state.tone.auto_exposure],
  ["s-zoom", (v) => (state.adjust.zoom = v), () => state.adjust.zoom],
  ["s-dy", (v) => (state.adjust.dy_mm = v), () => state.adjust.dy_mm],
  ["s-dx", (v) => (state.adjust.dx_mm = v), () => state.adjust.dx_mm],
  ["s-rot", (v) => (state.adjust.rotate_deg = v), () => state.adjust.rotate_deg],
];

function syncControls() {
  for (const [id, , get] of SLIDERS) $<HTMLInputElement>(id).value = String(get());
}

for (const [id, set] of SLIDERS) {
  $<HTMLInputElement>(id).addEventListener("input", (e) => {
    set(Number((e.target as HTMLInputElement).value));
    void rerender();
  });
}

$("btn-reset").onclick = () => {
  state.tone = { ...DEFAULT_TONE };
  state.adjust = { ...DEFAULT_ADJUST };
  syncControls();
  renderSwatches();
  void rerender();
};

// Latest-wins re-rendering: at most one request in flight, plus one queued.
let inflight = false;
let queued = false;
async function rerender() {
  if (inflight) {
    queued = true;
    return;
  }
  inflight = true;
  try {
    const opts = { adjust: state.adjust, dpi: PREVIEW_DPI };
    const [full, raw] = await Promise.all([render({ ...opts, tone: state.tone }), render({ ...opts, tone: NO_TONE })]);
    state.rendered = full.canvas;
    state.original = raw.canvas;
    state.info = full.info;
    paintResult();
  } catch (e) {
    toast(e instanceof Error ? e.message : String(e), true);
  } finally {
    inflight = false;
    if (queued) {
      queued = false;
      void rerender();
    }
  }
}

function paintResult() {
  const info = state.info;
  const src = $<HTMLInputElement>("show-original").checked ? state.original : state.rendered;
  if (!info || !src) return;
  const pv = $<HTMLCanvasElement>("preview");
  pv.width = src.width;
  pv.height = src.height;
  pv.getContext("2d")!.drawImage(src, 0, 0);
  drawGuides(info);

  const status = info.report.status;
  const v = $("verdict");
  v.className = `verdict ${status}`;
  const fails = info.report.checks.filter((c) => c.status === "fail");
  const title: Record<Status, string> = {
    pass: "Ready to use",
    warn: "Usable — review the warnings",
    fail: "Not compliant yet",
  };
  const sub: Record<Status, string> = {
    pass: `Meets the ${currentSpec().name} checks we can run automatically.`,
    warn: "Nothing blocking, but double-check the points marked below.",
    fail: fails.some((f) => ["expression", "eyes_open", "facing", "sharpness", "framing"].includes(f.id))
      ? "Some problems need a new photo — sliders can't fix them."
      : "Adjust the settings below to fix the remaining issues.",
  };
  v.innerHTML = `<div class="badge">${status === "pass" ? "✓" : "!"}</div><div><h3>${title[status]}</h3><p>${sub[status]}</p></div>`;

  $("checks").innerHTML = info.report.checks
    .map((c) => `<li class="${c.status}"><span class="m">${c.status === "pass" ? "✓" : c.status === "warn" ? "!" : "✗"}</span><span>${c.label}<small>${c.detail}</small></span></li>`)
    .join("");

  const spec = currentSpec();
  const notes = [...spec.notes];
  $("spec-notes").innerHTML = notes
    .map((n) => `<div class="${!spec.print_accepted && n.includes("printed") ? "alert" : ""}">${n}</div>`)
    .join("");

  const k = PHOTO_DPI / 25.4;
  $("dl-photo-sub").textContent = `${Math.round(spec.width_mm * k)}×${Math.round(spec.height_mm * k)} px · JPEG ${PHOTO_DPI} dpi`;
}

function drawGuides(info: RenderInfo) {
  const g = $<HTMLCanvasElement>("guides");
  const pv = $<HTMLCanvasElement>("preview");
  g.width = pv.width;
  g.height = pv.height;
  const ctx = g.getContext("2d")!;
  ctx.clearRect(0, 0, g.width, g.height);
  if (!$<HTMLInputElement>("show-guides").checked || !state.face) return;
  const spec = currentSpec();
  const k = g.width / spec.width_mm;
  // Chin position in frame mm via the inverse of the frame → source transform.
  const [a, b, c, d, e, f] = info.transform;
  const det = a * e - b * d;
  const chin = state.face.head.chin;
  const chinY = (-d * (chin.x - c) + a * (chin.y - f)) / det;
  ctx.lineWidth = 1.5;
  // Eye band.
  ctx.fillStyle = "rgba(245,183,74,.18)";
  const eTop = (spec.height_mm - spec.eyes_max_mm) * k, eBot = (spec.height_mm - spec.eyes_min_mm) * k;
  ctx.fillRect(0, eTop, g.width, eBot - eTop);
  // Allowed crown band given the chin.
  ctx.fillStyle = "rgba(73,227,196,.22)";
  const cTop = (chinY - spec.head_max_mm) * k, cBot = (chinY - spec.head_min_mm) * k;
  ctx.fillRect(g.width * 0.2, cTop, g.width * 0.6, cBot - cTop);
  ctx.strokeStyle = "#16a37f";
  ctx.setLineDash([5, 4]);
  ctx.beginPath();
  ctx.moveTo(g.width * 0.15, chinY * k);
  ctx.lineTo(g.width * 0.85, chinY * k);
  ctx.moveTo(g.width / 2, 0);
  ctx.lineTo(g.width / 2, g.height);
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.font = `${Math.round(g.width / 34)}px ui-monospace, monospace`;
  ctx.fillStyle = "#0f6e56";
  ctx.fillText("crown zone", g.width * 0.21, cTop - 4);
  ctx.fillText("chin", g.width * 0.86, chinY * k + 4);
  ctx.fillStyle = "#8a5a00";
  ctx.fillText("eyes", 6, eTop - 4);
}

$("show-guides").addEventListener("change", () => state.info && drawGuides(state.info));
$("show-original").addEventListener("change", paintResult);

// ------------------------------------------------------------- downloads ---

function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

async function download(kind: "photo" | "sheet") {
  const btn = $<HTMLButtonElement>(kind === "photo" ? "dl-photo" : "dl-sheet");
  btn.disabled = true;
  try {
    const r = await call<"exported">({
      type: "export",
      id: nextId++,
      kind,
      spec: state.spec,
      opts: { adjust: state.adjust, tone: state.tone },
      dpi: PHOTO_DPI,
      paper: state.paper,
      date: today(),
    });
    const blob = new Blob([r.jpeg as BlobPart], { type: "image/jpeg" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = kind === "photo" ? `id-photo-${state.spec}-${today()}.jpg` : `id-photo-sheet-${state.spec}-${state.paper}-${today()}.jpg`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
    if (kind === "sheet") toast(`Print at 100% (no “fit to page”) on ${state.paper === "4x6" ? "4×6 in" : "10×15 cm"} paper.`);
  } catch (e) {
    toast(e instanceof Error ? e.message : String(e), true);
  } finally {
    btn.disabled = false;
  }
}

$("dl-photo").onclick = () => download("photo");
$("dl-sheet").onclick = () => download("sheet");
$("paper-picker").addEventListener("click", (e) => {
  const b = (e.target as HTMLElement).closest("[data-paper]") as HTMLElement | null;
  if (!b) return;
  state.paper = b.dataset.paper as "10x15" | "4x6";
  $("paper-picker").querySelectorAll(".chip").forEach((c) => c.classList.toggle("on", c === b));
});
$("btn-restart").onclick = () => show("start");

// ----------------------------------------------------------------- input ---

$<HTMLInputElement>("file-input").addEventListener("change", async (e) => {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  try {
    const bmp = await createImageBitmap(file, { imageOrientation: "from-image" });
    const c = fitCanvas(bmp);
    bmp.close();
    await processPhoto(c);
  } catch (err) {
    toast(`Could not open this image: ${err instanceof Error ? err.message : err}`, true);
  }
});

const camera = new Camera();
$("btn-camera").onclick = async () => {
  show("camera");
  try {
    const shot = await camera.capture();
    if (shot) await processPhoto(shot);
    else show("start");
  } catch (e) {
    show("start");
    toast(e instanceof Error ? e.message : String(e), true);
  }
};
