// Messages between the UI and the processing worker.

export interface Adjust {
  dx_mm: number;
  dy_mm: number;
  zoom: number;
  rotate_deg: number;
}

export interface Tone {
  white_balance: number;
  /** -1 cooler … 1 warmer, on top of the automatic white balance. */
  temperature: number;
  auto_exposure: number;
  brightness_ev: number;
  background: number;
  background_color: [number, number, number] | null;
}

export interface RenderOpts {
  adjust?: Adjust;
  tone?: Tone;
  dpi?: number;
}

export interface Spec {
  id: string;
  name: string;
  width_mm: number;
  height_mm: number;
  head_min_mm: number;
  head_max_mm: number;
  eyes_min_mm: number;
  eyes_max_mm: number;
  background: [number, number, number];
  white_background_forbidden: boolean;
  digital_min_px: [number, number] | null;
  print_accepted: boolean;
  notes: string[];
}

export interface Pt {
  x: number;
  y: number;
}

export interface Face {
  bbox: [number, number, number, number];
  landmarks: number[];
  head: {
    chin: Pt;
    eyes: Pt;
    eye_r: Pt;
    eye_l: Pt;
    up: Pt;
    roll: number;
    skull_top: Pt;
    hair_top: Pt | null;
  };
}

export type Status = "pass" | "warn" | "fail";

export interface Check {
  id: string;
  label: string;
  status: Status;
  detail: string;
  /** Message key and numbers behind `detail`, for localisation. */
  msg: string;
  args: number[];
}

export interface RenderInfo {
  placement: {
    head_mm: number;
    eyes_from_bottom_mm: number;
    width_mm: number;
    height_mm: number;
    src_px_per_mm: number;
  };
  tone: { wb_gains: [number, number, number]; exposure_ev: number };
  report: { spec: string; status: Status; checks: Check[] };
  transform: [number, number, number, number, number, number];
}

/** A face found by the fast detector, in the coordinates of the sent frame. */
export interface Detection {
  bbox: { x: number; y: number; w: number; h: number };
  score: number;
  /** Image-left eye, image-right eye, nose, image-left mouth, image-right mouth. */
  kps: Pt[];
}

export interface LiveFaces {
  faces: Detection[];
  width: number;
  height: number;
  /** Mean luma (0..1) of the main face, and of its image-left / image-right halves. */
  light: { mean: number; left: number; right: number } | null;
}

export type Req =
  | { type: "init" }
  | { type: "detect"; id: number; bitmap: ImageBitmap }
  | { type: "locate"; bitmap: ImageBitmap }
  | { type: "segment"; width: number; height: number }
  | { type: "render"; id: number; spec: string; opts: RenderOpts }
  | {
      type: "export";
      id: number;
      kind: "photo" | "sheet";
      spec: string;
      opts: RenderOpts;
      dpi: number;
      paper: "10x15" | "4x6";
      date: string;
      /** Localised sheet text (see SheetText in photo-core). */
      strings: Record<string, string>;
    };

export type Res =
  | { type: "progress"; loaded: number }
  | { type: "ready"; specs: Spec[] }
  | ({ type: "detected"; id: number } & LiveFaces)
  | { type: "located"; face: Face; width: number; height: number }
  | { type: "segmented"; face: Face; matte: Uint8Array }
  | { type: "rendered"; id: number; width: number; height: number; rgba: Uint8Array; info: RenderInfo }
  | { type: "exported"; id: number; jpeg: Uint8Array }
  | { type: "error"; id?: number; step: string; message: string };
