// The "lab" animation shown while the photo is processed: scanlines sweep the
// image, overlays appear as each result arrives, then the view flies into the
// crop and successive corrected versions are revealed pass by pass.
import type { Face, Pt } from "./protocol";

type Source = HTMLCanvasElement | ImageBitmap;

/** 2D affine as canvas `setTransform` takes it: x' = a x + c y + e, y' = b x + d y + f. */
type M = [number, number, number, number, number, number];

const ACCENT = "#49e3c4";
const ACCENT_2 = "#6aa8ff";

interface Reveal {
  img: Source;
  /** 0 → nothing shown, 1 → fully shown; revealed top-down behind the scanline. */
  t: number;
  label?: string;
}

export class Stage {
  private ctx: CanvasRenderingContext2D;
  private dpr = Math.min(2, window.devicePixelRatio || 1);
  private w = 0;
  private h = 0;
  private raf = 0;

  source: Source | null = null;
  /** Source px → canvas px. */
  view: M = [1, 0, 0, 1, 0, 0];
  face: Face | null = null;
  bracket = 0; // 0..1 face-box animation
  meshShown = 0; // 0..1 fraction of landmarks visible
  matte: HTMLCanvasElement | null = null;
  matteT = 0; // 0..1 matte reveal
  measureT = 0; // 0..1 measurement lines
  scan: number | null = null; // scanline position 0..1 (null = hidden)
  scanLabel = "";
  /** Crop result layers, drawn over the frame rectangle. */
  frameRect: { x: number; y: number; w: number; h: number } | null = null;
  layers: Reveal[] = [];
  dim = 0; // darken outside the frame

  constructor(private canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
    new ResizeObserver(() => this.resize()).observe(canvas);
    this.resize();
    const loop = () => {
      this.draw();
      this.raf = requestAnimationFrame(loop);
    };
    this.raf = requestAnimationFrame(loop);
  }

  destroy() {
    cancelAnimationFrame(this.raf);
  }

  private resize() {
    const r = this.canvas.getBoundingClientRect();
    this.w = r.width;
    this.h = r.height;
    this.canvas.width = Math.round(r.width * this.dpr);
    this.canvas.height = Math.round(r.height * this.dpr);
    if (this.source && !this.frameRect) this.view = this.fitView();
  }

  get size() {
    return { w: this.w, h: this.h };
  }

  /** View that fits the whole source photo in the stage. */
  fitView(): M {
    if (!this.source) return [1, 0, 0, 1, 0, 0];
    const s = Math.min(this.w / this.source.width, this.h / this.source.height);
    return [s, 0, 0, s, (this.w - this.source.width * s) / 2, (this.h - this.source.height * s) / 2];
  }

  /**
   * View in which the spec frame (mm → src affine `t`) fills `rect` on stage.
   * canvas = rect.origin + (mm * k), mm = inverse(t)(src).
   */
  cropView(t: [number, number, number, number, number, number], widthMm: number, rect: { x: number; y: number; w: number }): M {
    const [a, b, c, d, e, f] = t; // src = (a*x + b*y + c, d*x + e*y + f)
    const det = a * e - b * d;
    const ia = e / det, ib = -b / det, id = -d / det, ie = a / det;
    const ic = -(ia * c + ib * f), iff = -(id * c + ie * f);
    const k = rect.w / widthMm;
    // canvas.x = rect.x + k*(ia*sx + ib*sy + ic), canvas.y = rect.y + k*(id*sx + ie*sy + iff)
    return [k * ia, k * id, k * ib, k * ie, rect.x + k * ic, rect.y + k * iff];
  }

  private apply(p: Pt): Pt {
    const [a, b, c, d, e, f] = this.view;
    return { x: a * p.x + c * p.y + e, y: b * p.x + d * p.y + f };
  }

  private draw() {
    const { ctx, dpr } = this;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#050607";
    ctx.fillRect(0, 0, this.w, this.h);
    if (!this.source) return;

    // Base photo.
    const [a, b, c, d, e, f] = this.view;
    ctx.save();
    ctx.setTransform(dpr * a, dpr * b, dpr * c, dpr * d, dpr * e, dpr * f);
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(this.source, 0, 0);
    if (this.matte && this.matteT > 0) this.drawMatte();
    ctx.restore();

    if (this.dim > 0 && this.frameRect) {
      const r = this.frameRect;
      ctx.fillStyle = `rgba(5,6,7,${0.82 * this.dim})`;
      ctx.beginPath();
      ctx.rect(0, 0, this.w, this.h);
      ctx.rect(r.x, r.y, r.w, r.h);
      ctx.fill("evenodd");
      ctx.strokeStyle = `rgba(73,227,196,${this.dim})`;
      ctx.lineWidth = 1.5;
      ctx.strokeRect(r.x - 0.5, r.y - 0.5, r.w + 1, r.h + 1);
    }

    for (const l of this.layers) this.drawLayer(l);
    if (this.face) this.drawFace(this.face);
    if (this.scan !== null) this.drawScan(this.scan);
  }

  private drawMatte() {
    // Background tinted and rasterised with horizontal lines, revealed top-down.
    const { ctx } = this;
    const src = this.source!;
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, 0, src.width, src.height * this.matteT);
    ctx.clip();
    ctx.globalAlpha = 0.85;
    ctx.drawImage(this.matte!, 0, 0, src.width, src.height);
    ctx.restore();
  }

  private drawLayer(l: Reveal) {
    if (!this.frameRect || l.t <= 0) return;
    const { ctx } = this;
    const r = this.frameRect;
    ctx.save();
    ctx.beginPath();
    ctx.rect(r.x, r.y, r.w, r.h * l.t);
    ctx.clip();
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(l.img, r.x, r.y, r.w, r.h);
    ctx.restore();
    if (l.t > 0 && l.t < 1) {
      const y = r.y + r.h * l.t;
      ctx.fillStyle = ACCENT;
      ctx.fillRect(r.x, y - 1, r.w, 2);
    }
    if (l.label && l.t > 0.05) {
      ctx.font = "600 11px ui-monospace, Menlo, monospace";
      ctx.fillStyle = "rgba(5,6,7,.75)";
      const tw = ctx.measureText(l.label).width;
      ctx.fillRect(r.x + 8, r.y + 8, tw + 12, 20);
      ctx.fillStyle = ACCENT;
      ctx.fillText(l.label, r.x + 14, r.y + 22);
    }
  }

  private drawFace(face: Face) {
    const { ctx } = this;
    // Box corners snapping in.
    if (this.bracket > 0) {
      const [bx, by, bw, bh] = face.bbox;
      const p0 = this.apply({ x: bx, y: by });
      const p1 = this.apply({ x: bx + bw, y: by + bh });
      const grow = 1 + (1 - ease(this.bracket)) * 0.6;
      const cx = (p0.x + p1.x) / 2, cy = (p0.y + p1.y) / 2;
      const hw = ((p1.x - p0.x) / 2) * grow, hh = ((p1.y - p0.y) / 2) * grow;
      const len = Math.min(hw, hh) * 0.35;
      ctx.strokeStyle = ACCENT;
      ctx.globalAlpha = Math.min(1, this.bracket * 2) * (1 - this.dim);
      ctx.lineWidth = 2;
      for (const [sx, sy] of [[-1, -1], [1, -1], [1, 1], [-1, 1]]) {
        const x = cx + sx * hw, y = cy + sy * hh;
        ctx.beginPath();
        ctx.moveTo(x, y - sy * -len);
        ctx.lineTo(x, y);
        ctx.lineTo(x - sx * len, y);
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
    }
    // Landmarks appear in mesh order.
    if (this.meshShown > 0) {
      const n = Math.floor((face.landmarks.length / 2) * this.meshShown);
      ctx.fillStyle = ACCENT;
      ctx.globalAlpha = 0.85 * (1 - this.dim);
      for (let i = 0; i < n; i++) {
        const p = this.apply({ x: face.landmarks[2 * i], y: face.landmarks[2 * i + 1] });
        ctx.fillRect(p.x - 0.9, p.y - 0.9, 1.8, 1.8);
      }
      ctx.globalAlpha = 1;
    }
    if (this.measureT > 0) this.drawMeasures(face);
  }

  private drawMeasures(face: Face) {
    const { ctx } = this;
    const h = face.head;
    const t = ease(this.measureT);
    const across = { x: -h.up.y, y: h.up.x };
    const lineAt = (p: Pt, color: string, label: string, half: number) => {
      const a = this.apply({ x: p.x - across.x * half * t, y: p.y - across.y * half * t });
      const b = this.apply({ x: p.x + across.x * half * t, y: p.y + across.y * half * t });
      ctx.strokeStyle = color;
      ctx.lineWidth = 1.5;
      ctx.setLineDash([6, 4]);
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.font = "600 11px ui-monospace, Menlo, monospace";
      ctx.fillStyle = color;
      ctx.globalAlpha = t;
      ctx.fillText(label, b.x + 6, b.y + 4);
      ctx.globalAlpha = 1;
    };
    const fw = Math.hypot(h.eye_l.x - h.eye_r.x, h.eye_l.y - h.eye_r.y) * 1.6;
    ctx.globalAlpha = 1 - this.dim;
    lineAt(h.skull_top, ACCENT, "CROWN", fw);
    if (h.hair_top) lineAt(h.hair_top, ACCENT_2, "HAIR", fw * 0.8);
    lineAt(h.eyes, "#f5b74a", "EYES", fw);
    lineAt(h.chin, ACCENT, "CHIN", fw);
    // Vertical axis with the head-height bracket.
    const off = { x: across.x * fw * 1.15, y: across.y * fw * 1.15 };
    const a = this.apply({ x: h.skull_top.x - off.x, y: h.skull_top.y - off.y });
    const b = this.apply({ x: h.chin.x - off.x, y: h.chin.y - off.y });
    ctx.strokeStyle = ACCENT;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  private drawScan(pos: number) {
    const { ctx } = this;
    // Region to scan: the frame when cropped, else the photo's bounds.
    let x0 = 0, y0 = 0, x1 = this.w, y1 = this.h;
    if (this.frameRect) {
      ({ x: x0, y: y0 } = this.frameRect);
      x1 = x0 + this.frameRect.w;
      y1 = y0 + this.frameRect.h;
    } else if (this.source) {
      const p0 = this.apply({ x: 0, y: 0 });
      const p1 = this.apply({ x: this.source.width, y: this.source.height });
      x0 = Math.max(0, p0.x); y0 = Math.max(0, p0.y); x1 = Math.min(this.w, p1.x); y1 = Math.min(this.h, p1.y);
    }
    const y = y0 + (y1 - y0) * pos;
    // Trailing band of raster lines.
    const band = (y1 - y0) * 0.18;
    const g = ctx.createLinearGradient(0, y - band, 0, y);
    g.addColorStop(0, "rgba(73,227,196,0)");
    g.addColorStop(1, "rgba(73,227,196,0.22)");
    ctx.fillStyle = g;
    ctx.fillRect(x0, y - band, x1 - x0, band);
    ctx.fillStyle = "rgba(73,227,196,0.10)";
    for (let ly = y - band; ly < y; ly += 3) ctx.fillRect(x0, ly, x1 - x0, 1);
    // Bright line with glow.
    ctx.shadowColor = ACCENT;
    ctx.shadowBlur = 18;
    ctx.fillStyle = "#c9fff3";
    ctx.fillRect(x0, y - 1, x1 - x0, 2);
    ctx.shadowBlur = 0;
    if (this.scanLabel) {
      ctx.font = "600 10px ui-monospace, Menlo, monospace";
      ctx.fillStyle = ACCENT;
      ctx.fillText(this.scanLabel, x0 + 8, Math.max(y0 + 12, y - 6));
    }
  }
}

export const ease = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);

/** Animate `fn(t)` for t in 0..1 over `ms`. */
export function tween(ms: number, fn: (t: number) => void): Promise<void> {
  return new Promise((resolve) => {
    const start = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / ms);
      fn(t);
      if (t < 1) requestAnimationFrame(step);
      else resolve();
    };
    requestAnimationFrame(step);
  });
}

/** Sweep the scanline repeatedly until `work` settles (at least `minMs`). */
export async function scanWhile<T>(stage: Stage, label: string, work: Promise<T>, minMs = 1200, period = 1400): Promise<T> {
  let done = false;
  const start = performance.now();
  work.then(() => (done = true), () => (done = true));
  stage.scanLabel = label;
  await new Promise<void>((resolve) => {
    const step = (now: number) => {
      const el = now - start;
      const cycle = (el % period) / period;
      stage.scan = ease(cycle);
      // Finish on a full sweep once the work is done.
      if (done && el > minMs && cycle > 0.94) {
        stage.scan = null;
        resolve();
        return;
      }
      requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  });
  stage.scanLabel = "";
  return work;
}

/** Interpolate two views by scale and rotation, keeping `pivot` (source px) on a straight path. */
export function lerpView(v0: M, v1: M, t: number, pivot: Pt): M {
  const s0 = Math.hypot(v0[0], v0[1]), s1 = Math.hypot(v1[0], v1[1]);
  const r0 = Math.atan2(v0[1], v0[0]), r1 = Math.atan2(v1[1], v1[0]);
  const s = s0 * Math.pow(s1 / s0, t);
  const r = r0 + (r1 - r0) * t;
  const at = (v: M) => ({ x: v[0] * pivot.x + v[2] * pivot.y + v[4], y: v[1] * pivot.x + v[3] * pivot.y + v[5] });
  const p0 = at(v0), p1 = at(v1);
  const px = p0.x + (p1.x - p0.x) * t, py = p0.y + (p1.y - p0.y) * t;
  const a = s * Math.cos(r), b = s * Math.sin(r);
  // [a -b; b a] * pivot + (e, f) = (px, py)
  return [a, b, -b, a, px - (a * pivot.x - b * pivot.y), py - (b * pivot.x + a * pivot.y)];
}
