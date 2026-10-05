// Camera capture with an oval face guide, rotating rule reminders and live
// hints (position, distance, tilt, lighting) from the face detector.
import { t, type Key } from "./i18n";
import type { LiveFaces } from "./protocol";

const rules = () => t("cam.rules").split("\n");

/** Width of frames sent to the detector. */
const DETECT_WIDTH = 480;

export type Detector = (bitmap: ImageBitmap) => Promise<LiveFaces>;

interface Oval {
  cx: number;
  cy: number;
  rx: number;
  ry: number;
}

type Hint = { key: Key; ok: boolean; arrow?: "left" | "right" | "up" | "down" };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class Camera {
  private video = $<HTMLVideoElement>("video");
  private oval = document.getElementById("oval") as unknown as SVGSVGElement;
  private ticker = $<HTMLDivElement>("ticker");
  private hintEl = $<HTMLDivElement>("cam-hint");
  private stream: MediaStream | null = null;
  private facing: "user" | "environment" = "user";
  /** Mirror the preview: true for front cameras (decided from the opened track). */
  private mirrored = true;
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
    $("cam-flip").addEventListener("click", () => this.flip().catch((e) => console.error("camera switch failed", e)));
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
    this.ticker.textContent = rules()[0];
    this.tickTimer = window.setInterval(() => {
      this.ticker.style.opacity = "0";
      window.setTimeout(() => {
        const r = rules();
        i = (i + 1) % r.length;
        this.ticker.textContent = r[i];
        this.ticker.style.opacity = "1";
      }, 300);
    }, 3200);
    this.showHint({ key: "hint.looking", ok: false });
    void this.hintLoop();
    return new Promise((resolve) => (this.resolve = resolve));
  }

  private async start(video: MediaTrackConstraints = { facingMode: this.facing }) {
    this.stopStream();
    if (!navigator.mediaDevices?.getUserMedia) {
      throw new Error(t("cam.unavailable"));
    }
    this.stream = await navigator.mediaDevices.getUserMedia({
      video: { width: { ideal: 2560 }, height: { ideal: 2560 }, ...video },
      audio: false,
    });
    this.applyTrack();
    this.video.srcObject = this.stream;
    await this.video.play().catch(() => {});
    this.drawOval();
    // Only offer switching when there is something to switch to.
    const cams = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "videoinput");
    $("cam-flip").style.visibility = cams.length > 1 ? "visible" : "hidden";
  }

  /** Read which camera actually opened and mirror only front cameras. */
  private applyTrack() {
    const track = this.stream?.getVideoTracks()[0];
    if (!track) return;
    const s = track.getSettings();
    let facing = s.facingMode as string | undefined;
    if (!facing) {
      const label = track.label.toLowerCase();
      if (/back|rear|environment|arrière|rück/.test(label)) facing = "environment";
      else if (/front|user|facetime|avant|vorder/.test(label)) facing = "user";
    }
    // Laptops' webcams report nothing: treat them as front cameras.
    this.facing = facing === "environment" ? "environment" : "user";
    this.mirrored = this.facing === "user";
    this.video.classList.toggle("mirror", this.mirrored);
  }

  /** Switch between front and back cameras. */
  private async flip() {
    const current = this.stream?.getVideoTracks()[0]?.getSettings().deviceId;
    const want = this.facing === "user" ? "environment" : "user";
    try {
      // A hard constraint, so the browser can't hand back the same camera.
      await this.start({ facingMode: { exact: want } });
      if (this.stream?.getVideoTracks()[0]?.getSettings().deviceId !== current) return;
    } catch {
      /* no camera with that facing mode reported: fall back to device ids */
    }
    const cams = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "videoinput");
    if (cams.length < 2) {
      await this.start({ deviceId: current ? { exact: current } : undefined });
      return;
    }
    const i = cams.findIndex((d) => d.deviceId === current);
    const next = cams[(i + 1) % cams.length];
    await this.start({ deviceId: { exact: next.deviceId } });
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
          this.showHint({ key: "hint.loading", ok: false });
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
    if (this.mirrored) sx = W - sx; // mirrored preview
    return { x: sx, y: oy + y * s, s };
  }

  private evaluate(r: LiveFaces): Hint {
    if (!r.faces.length) return { key: "hint.no_face", ok: false };
    const main = r.faces[0];
    const area = (f: typeof main) => f.bbox.w * f.bbox.h;
    if (r.faces.filter((f) => area(f) > area(main) * 0.2).length > 1) {
      return { key: "hint.several", ok: false };
    }
    const { cx, cy, ry } = this.geom;
    const b = main.bbox;
    const c = this.toScreen(b.x + b.w / 2, b.y + b.h * 0.55, r.width, r.height);
    const faceH = b.h * c.s;
    // YuNet's box spans brows to chin: about 0.72 of the oval (crown to chin).
    const size = faceH / (2 * ry * 0.72);
    if (size < 0.82) return { key: "hint.closer", ok: false };
    if (size > 1.2) return { key: "hint.back", ok: false };
    const dx = (c.x - cx) / (2 * ry * 0.74), dy = (c.y - cy) / (2 * ry);
    if (dx > 0.1) return { key: "hint.left", ok: false, arrow: "left" };
    if (dx < -0.1) return { key: "hint.right", ok: false, arrow: "right" };
    if (dy > 0.08) return { key: "hint.up", ok: false, arrow: "up" };
    if (dy < -0.08) return { key: "hint.down", ok: false, arrow: "down" };
    const [le, re, nose] = main.kps;
    const roll = (Math.atan2(re.y - le.y, re.x - le.x) * 180) / Math.PI;
    if (Math.abs(roll) > 6) return { key: "hint.straight", ok: false };
    const eyeDist = Math.hypot(re.x - le.x, re.y - le.y);
    const yaw = (nose.x - (le.x + re.x) / 2) / eyeDist;
    if (Math.abs(yaw) > 0.18) return { key: "hint.face", ok: false };
    if (r.light) {
      if (r.light.mean < 0.22) return { key: "hint.dark", ok: false };
      if (r.light.mean > 0.9) return { key: "hint.bright", ok: false };
      const uneven = Math.abs(r.light.left - r.light.right) / r.light.mean;
      if (uneven > 0.35) return { key: "hint.uneven", ok: false };
    }
    return { key: "hint.ok", ok: true };
  }

  private showHint(h: Hint) {
    // Debounce changes so the hint doesn't flicker between states.
    const now = performance.now();
    if (h.key !== this.lastHint) {
      this.lastHint = h.key;
      this.hintSince = now;
    }
    if (now - this.hintSince < 250 && this.hintEl.textContent) return;
    const arrows = { left: "←", right: "→", up: "↑", down: "↓" };
    this.hintEl.innerHTML = h.arrow ? `<span class="arrow">${arrows[h.arrow]}</span>${t(h.key)}` : t(h.key);
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
