// Camera capture with an oval face guide, rotating rule reminders and live
// hints (position, distance, tilt, lighting) from the face detector.
import type { LiveFaces } from "./protocol";

const RULES = [
  "Neutral face — mouth closed, no smile",
  "Look straight into the camera",
  "Plain, light background behind you",
  "Even light, no shadows on the face",
  "Eyes open and visible, no glare",
  "Hair away from your eyes and face",
  "Camera at eye level, about 1 m away",
];

/** Width of frames sent to the detector. */
const DETECT_WIDTH = 480;

export type Detector = (bitmap: ImageBitmap) => Promise<LiveFaces>;

interface Oval {
  cx: number;
  cy: number;
  rx: number;
  ry: number;
}

type Hint = { text: string; ok: boolean; arrow?: "left" | "right" | "up" | "down" };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class Camera {
  private video = $<HTMLVideoElement>("video");
  private oval = document.getElementById("oval") as unknown as SVGSVGElement;
  private ticker = $<HTMLDivElement>("ticker");
  private hintEl = $<HTMLDivElement>("cam-hint");
  private stream: MediaStream | null = null;
  private facing: "user" | "environment" = "user";
  private timer = false;
  private tickTimer = 0;
  private active = false;
  private geom: Oval = { cx: 0, cy: 0, rx: 1, ry: 1 };
  private resolve: ((c: HTMLCanvasElement | null) => void) | null = null;
  private onResize = () => this.drawOval();
  /** Recent hint keys, so a hint must persist briefly before it shows. */
  private lastHint = "";
  private hintSince = 0;

  constructor(private detect: Detector) {
    $("cam-shutter").addEventListener("click", () => this.shoot());
    $("cam-cancel").addEventListener("click", () => this.finish(null));
    $("cam-flip").addEventListener("click", () => {
      this.facing = this.facing === "user" ? "environment" : "user";
      void this.start();
    });
    const t = $("cam-timer");
    t.addEventListener("click", () => {
      this.timer = !this.timer;
      t.setAttribute("aria-pressed", String(this.timer));
    });
  }

  /** Opens the camera; resolves with the captured frame, or null if cancelled. */
  async capture(): Promise<HTMLCanvasElement | null> {
    await this.start();
    this.active = true;
    window.addEventListener("resize", this.onResize);
    let i = 0;
    this.ticker.textContent = RULES[0];
    this.tickTimer = window.setInterval(() => {
      this.ticker.style.opacity = "0";
      window.setTimeout(() => {
        i = (i + 1) % RULES.length;
        this.ticker.textContent = RULES[i];
        this.ticker.style.opacity = "1";
      }, 300);
    }, 3200);
    this.showHint({ text: "Looking for your face…", ok: false });
    void this.hintLoop();
    return new Promise((resolve) => (this.resolve = resolve));
  }

  private async start() {
    this.stopStream();
    if (!navigator.mediaDevices?.getUserMedia) {
      throw new Error("Camera not available. On a phone, open this page over HTTPS.");
    }
    this.stream = await navigator.mediaDevices.getUserMedia({
      video: { facingMode: this.facing, width: { ideal: 2560 }, height: { ideal: 2560 } },
      audio: false,
    });
    this.video.srcObject = this.stream;
    this.video.classList.toggle("mirror", this.facing === "user");
    await this.video.play().catch(() => {});
    this.drawOval();
  }

  private drawOval() {
    const w = window.innerWidth, h = window.innerHeight;
    // Face oval sized like a head at passport framing: ~70% of a 35×45 frame.
    const frameH = Math.min(h * 0.62, w * 0.9 * (45 / 35));
    const ry = frameH * 0.38, rx = ry * 0.74;
    const cx = w / 2, cy = h * 0.44;
    this.geom = { cx, cy, rx, ry };
    this.oval.setAttribute("viewBox", `0 0 ${w} ${h}`);
    this.oval.innerHTML = `
      <defs><mask id="hole"><rect width="${w}" height="${h}" fill="#fff"/>
        <ellipse cx="${cx}" cy="${cy}" rx="${rx}" ry="${ry}" fill="#000"/></mask></defs>
      <rect width="${w}" height="${h}" fill="rgba(5,6,7,.55)" mask="url(#hole)" stroke="none"/>
      <ellipse class="guide" cx="${cx}" cy="${cy}" rx="${rx}" ry="${ry}"/>
      <line x1="${cx - rx * 0.55}" x2="${cx + rx * 0.55}" y1="${cy - ry * 0.12}" y2="${cy - ry * 0.12}"
        stroke="rgba(255,255,255,.35)" stroke-dasharray="4 6"/>
      <text x="${cx + rx * 0.6}" y="${cy - ry * 0.12 + 4}" fill="rgba(255,255,255,.6)" font-size="11"
        stroke="none" font-family="ui-monospace,monospace">eyes</text>`;
  }

  private async hintLoop() {
    while (this.active) {
      const started = performance.now();
      const v = this.video;
      if (v.videoWidth && v.readyState >= 2) {
        try {
          const scale = DETECT_WIDTH / v.videoWidth;
          const bitmap = await createImageBitmap(v, {
            resizeWidth: DETECT_WIDTH,
            resizeHeight: Math.round(v.videoHeight * scale),
            resizeQuality: "low",
          });
          const res = await this.detect(bitmap);
          if (this.active) this.showHint(this.evaluate(res));
        } catch {
          // Detector not ready yet (models still loading): keep the rules only.
          this.showHint({ text: "Loading face detector…", ok: false });
        }
      }
      // ~6 checks per second at most.
      await new Promise((r) => setTimeout(r, Math.max(0, 160 - (performance.now() - started))));
    }
  }

  /** Map a point in detector-frame coordinates to screen coordinates. */
  private toScreen(x: number, y: number, frameW: number, frameH: number) {
    const W = window.innerWidth, H = window.innerHeight;
    const s = Math.max(W / frameW, H / frameH); // object-fit: cover
    const ox = (W - frameW * s) / 2, oy = (H - frameH * s) / 2;
    let sx = ox + x * s;
    if (this.facing === "user") sx = W - sx; // mirrored preview
    return { x: sx, y: oy + y * s, s };
  }

  private evaluate(r: LiveFaces): Hint {
    if (!r.faces.length) return { text: "Can't see a face — look at the camera", ok: false };
    const main = r.faces[0];
    const area = (f: typeof main) => f.bbox.w * f.bbox.h;
    if (r.faces.filter((f) => area(f) > area(main) * 0.2).length > 1) {
      return { text: "Only one person in the picture", ok: false };
    }
    const { cx, cy, ry } = this.geom;
    const b = main.bbox;
    const c = this.toScreen(b.x + b.w / 2, b.y + b.h * 0.55, r.width, r.height);
    const faceH = b.h * c.s;
    // YuNet's box spans brows to chin: about 0.72 of the oval (crown to chin).
    const size = faceH / (2 * ry * 0.72);
    if (size < 0.82) return { text: "Move closer", ok: false };
    if (size > 1.2) return { text: "Move back a little", ok: false };
    const dx = (c.x - cx) / (2 * ry * 0.74), dy = (c.y - cy) / (2 * ry);
    if (dx > 0.1) return { text: "Move left", ok: false, arrow: "left" };
    if (dx < -0.1) return { text: "Move right", ok: false, arrow: "right" };
    if (dy > 0.08) return { text: "Move up", ok: false, arrow: "up" };
    if (dy < -0.08) return { text: "Move down", ok: false, arrow: "down" };
    const [le, re, nose] = main.kps;
    const roll = (Math.atan2(re.y - le.y, re.x - le.x) * 180) / Math.PI;
    if (Math.abs(roll) > 6) return { text: "Keep your head straight", ok: false };
    const eyeDist = Math.hypot(re.x - le.x, re.y - le.y);
    const yaw = (nose.x - (le.x + re.x) / 2) / eyeDist;
    if (Math.abs(yaw) > 0.18) return { text: "Turn to face the camera", ok: false };
    if (r.light) {
      if (r.light.mean < 0.22) return { text: "Too dark — find more light", ok: false };
      if (r.light.mean > 0.9) return { text: "Too bright — avoid direct light", ok: false };
      const uneven = Math.abs(r.light.left - r.light.right) / r.light.mean;
      if (uneven > 0.35) return { text: "Light is uneven — face the light", ok: false };
    }
    return { text: "Great — hold still", ok: true };
  }

  private showHint(h: Hint) {
    // Debounce changes so the hint doesn't flicker between states.
    const now = performance.now();
    if (h.text !== this.lastHint) {
      this.lastHint = h.text;
      this.hintSince = now;
    }
    if (now - this.hintSince < 250 && this.hintEl.textContent) return;
    const arrows = { left: "←", right: "→", up: "↑", down: "↓" };
    this.hintEl.innerHTML = h.arrow ? `<span class="arrow">${arrows[h.arrow]}</span>${h.text}` : h.text;
    this.hintEl.classList.toggle("ok", h.ok);
    this.oval.classList.toggle("ok", h.ok);
    $("cam-shutter").classList.toggle("ready", h.ok);
  }

  private async shoot() {
    if (this.timer) {
      const cd = $("countdown");
      for (const n of [3, 2, 1]) {
        cd.textContent = String(n);
        await new Promise((r) => setTimeout(r, 900));
      }
      cd.textContent = "";
    }
    const v = this.video;
    if (!v.videoWidth) return;
    const c = document.createElement("canvas");
    c.width = v.videoWidth;
    c.height = v.videoHeight;
    // The preview is mirrored for comfort; the captured photo is not.
    c.getContext("2d")!.drawImage(v, 0, 0);
    const flash = $("flash");
    flash.classList.remove("go");
    void flash.offsetWidth;
    flash.classList.add("go");
    await new Promise((r) => setTimeout(r, 250));
    this.finish(c);
  }

  private finish(c: HTMLCanvasElement | null) {
    this.active = false;
    window.clearInterval(this.tickTimer);
    window.removeEventListener("resize", this.onResize);
    this.hintEl.textContent = "";
    this.stopStream();
    this.resolve?.(c);
    this.resolve = null;
  }

  private stopStream() {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
  }
}
