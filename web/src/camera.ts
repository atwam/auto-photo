// Camera capture with an oval face guide and rotating rule reminders.

const RULES = [
  "Neutral face — mouth closed, no smile",
  "Look straight into the camera",
  "Fit your face inside the oval",
  "Plain, light background behind you",
  "Even light, no shadows on the face",
  "Eyes open and visible, no glare",
  "Hair away from your eyes and face",
  "Camera at eye level, about 1 m away",
];

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class Camera {
  private video = $<HTMLVideoElement>("video");
  private oval = document.getElementById("oval") as unknown as SVGSVGElement;
  private ticker = $<HTMLDivElement>("ticker");
  private stream: MediaStream | null = null;
  private facing: "user" | "environment" = "user";
  private timer = false;
  private tickTimer = 0;
  private resolve: ((c: HTMLCanvasElement | null) => void) | null = null;
  private onResize = () => this.drawOval();

  constructor() {
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
    }, 2800);
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
    window.clearInterval(this.tickTimer);
    window.removeEventListener("resize", this.onResize);
    this.stopStream();
    this.resolve?.(c);
    this.resolve = null;
  }

  private stopStream() {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
  }
}
