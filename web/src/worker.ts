// Runs the wasm pipeline off the main thread. Photos never leave the device:
// the only network requests are for the model files, cached after first use.
import init, { Engine, Portrait, specs } from "./pkg/photo_wasm.js";
import type { Detection, Req, Res } from "./protocol";

const MODEL_CACHE = "auto-photo-models-v1";
const MAX_SIDE = 2400;

let engine: Engine | null = null;
let portrait: Portrait | null = null;

const post = (msg: Res, transfer: Transferable[] = []) =>
  (self as unknown as Worker).postMessage(msg, transfer);

async function fetchModel(name: string, onProgress: (loaded: number) => void): Promise<Uint8Array> {
  const url = `${import.meta.env.BASE_URL}models/${name}`;
  let cache: Cache | null = null;
  try {
    cache = await caches.open(MODEL_CACHE);
    const hit = await cache.match(url);
    if (hit) return new Uint8Array(await hit.arrayBuffer());
  } catch {
    cache = null; // Cache API unavailable (private mode, file://…)
  }
  const res = await fetch(url);
  if (!res.ok || !res.body) throw new Error(`could not download ${name}`);
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    loaded += value.length;
    onProgress(loaded);
  }
  const bytes = new Uint8Array(loaded);
  let off = 0;
  for (const c of chunks) {
    bytes.set(c, off);
    off += c.length;
  }
  try {
    await cache?.put(url, new Response(bytes.slice()));
  } catch {
    /* quota: ignore */
  }
  return bytes;
}

/** Brightness of the face's central area, overall and per side (for lighting hints). */
function faceLight(data: Uint8ClampedArray, w: number, h: number, f: Detection) {
  const { x, y, w: bw, h: bh } = f.bbox;
  const x0 = Math.max(0, Math.round(x + bw * 0.15)), x1 = Math.min(w, Math.round(x + bw * 0.85));
  const y0 = Math.max(0, Math.round(y + bh * 0.25)), y1 = Math.min(h, Math.round(y + bh * 0.8));
  const mid = (x0 + x1) / 2;
  let l = 0, nl = 0, r = 0, nr = 0;
  for (let yy = y0; yy < y1; yy += 2) {
    for (let xx = x0; xx < x1; xx += 2) {
      const i = (yy * w + xx) * 4;
      const v = (0.2126 * data[i] + 0.7152 * data[i + 1] + 0.0722 * data[i + 2]) / 255;
      if (xx < mid) { l += v; nl++; } else { r += v; nr++; }
    }
  }
  if (!nl || !nr) return null;
  return { mean: (l + r) / (nl + nr), left: l / nl, right: r / nr };
}

async function handle(req: Req) {
  switch (req.type) {
    case "init": {
      await init();
      const names = ["yunet.onnx", "face_mesh.onnx", "modnet.onnx"] as const;
      const loaded = [0, 0, 0];
      const bytes = await Promise.all(
        names.map((n, i) =>
          fetchModel(n, (l) => {
            loaded[i] = l;
            post({ type: "progress", loaded: loaded.reduce((a, b) => a + b, 0) });
          }),
        ),
      );
      engine = new Engine(bytes[0], bytes[1], bytes[2]);
      post({ type: "ready", specs: specs() });
      break;
    }
    case "detect": {
      if (!engine) throw new Error("engine not ready");
      const { bitmap } = req;
      const { width: w, height: h } = bitmap;
      const canvas = new OffscreenCanvas(w, h);
      const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
      ctx.drawImage(bitmap, 0, 0);
      bitmap.close();
      const data = ctx.getImageData(0, 0, w, h).data;
      const faces = engine.detect(new Uint8Array(data.buffer), w, h) as Detection[];
      post({ type: "detected", id: req.id, faces, width: w, height: h, light: faces[0] ? faceLight(data, w, h, faces[0]) : null });
      break;
    }
    case "locate": {
      if (!engine) throw new Error("engine not ready");
      const { bitmap } = req;
      const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
      const w = Math.round(bitmap.width * scale);
      const h = Math.round(bitmap.height * scale);
      const canvas = new OffscreenCanvas(w, h);
      const ctx = canvas.getContext("2d")!;
      ctx.imageSmoothingQuality = "high";
      ctx.drawImage(bitmap, 0, 0, w, h);
      bitmap.close();
      const data = ctx.getImageData(0, 0, w, h).data;
      portrait?.free();
      portrait = null;
      portrait = engine.locate(new Uint8Array(data.buffer), w, h);
      post({ type: "located", face: portrait.face(), width: w, height: h });
      break;
    }
    case "segment": {
      if (!engine || !portrait) throw new Error("no photo");
      portrait.segment(engine);
      const matte = portrait.matte(req.width, req.height);
      post({ type: "segmented", face: portrait.face(), matte }, [matte.buffer]);
      break;
    }
    case "render": {
      if (!portrait) throw new Error("no photo");
      const r = portrait.render(req.spec, req.opts);
      const rgba = r.rgba();
      post({ type: "rendered", id: req.id, width: r.width, height: r.height, rgba, info: JSON.parse(r.info) }, [
        rgba.buffer,
      ]);
      r.free();
      break;
    }
    case "export": {
      if (!portrait) throw new Error("no photo");
      const r =
        req.kind === "sheet"
          ? portrait.sheet(req.spec, { ...req.opts, dpi: 300 }, req.paper, req.date)
          : portrait.render(req.spec, { ...req.opts, dpi: req.dpi });
      const jpeg = r.jpeg(95, req.kind === "sheet" ? 300 : req.dpi);
      r.free();
      post({ type: "exported", id: req.id, jpeg }, [jpeg.buffer]);
      break;
    }
  }
}

self.onmessage = async (e: MessageEvent<Req>) => {
  try {
    await handle(e.data);
  } catch (err) {
    const id = "id" in e.data ? e.data.id : undefined;
    post({ type: "error", id, step: e.data.type, message: err instanceof Error ? err.message : String(err) });
  }
};
