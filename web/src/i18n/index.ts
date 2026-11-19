// Runtime i18n: language from the URL (/fr/, /de/, /es/; English at /),
// static markup via data-i18n attributes, dynamic text via t().
import { LANGS, MESSAGES, format, langPath, type Key, type Lang } from "./messages";

export { LANGS, type Key, type Lang } from "./messages";
export { LANG_NAMES, langPath } from "./messages";

const PREF = "lang";
// Where the site is mounted ("/" or e.g. "/auto-photo/" on GitHub Pages).
const BASE = import.meta.env.BASE_URL;
let current: Lang = "en";

export const lang = () => current;

export function t(key: Key, params?: Record<string, string | number>): string {
  return format(MESSAGES[current][key] ?? MESSAGES.en[key], params);
}

/** Plural-aware lookup: `${base}_one` / `${base}_other`. */
export function tn(base: "step.note.problems" | "step.note.warnings", n: number): string {
  const rule = new Intl.PluralRules(current).select(n) === "one" ? "one" : "other";
  return t(`${base}_${rule}` as Key, { n });
}

/** Localised number with a fixed number of decimals. */
export function num(v: number, digits = 1): string {
  return new Intl.NumberFormat(current, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(v);
}

function fromPath(): Lang | null {
  const path = location.pathname;
  const rest = path.startsWith(BASE) ? path.slice(BASE.length) : path;
  const seg = rest.split("/").filter(Boolean)[0];
  return (LANGS as readonly string[]).includes(seg) ? (seg as Lang) : null;
}

function readPref(): Lang | null {
  try {
    const v = localStorage.getItem(PREF);
    return (LANGS as readonly string[]).includes(v ?? "") ? (v as Lang) : null;
  } catch {
    return null;
  }
}

/**
 * Language for this page load. At the root, a saved choice or the browser's
 * language wins and the URL is updated (no reload).
 */
export function initLang(): Lang {
  let l = fromPath();
  if (!l) {
    l = readPref();
    if (!l) {
      const nav = navigator.languages?.map((x) => x.slice(0, 2).toLowerCase()) ?? [];
      l = (nav.find((x) => (LANGS as readonly string[]).includes(x)) as Lang | undefined) ?? "en";
    }
    if (l !== "en") history.replaceState(null, "", langPath(l, BASE) + location.search + location.hash);
  }
  current = l;
  applyStatic();
  return l;
}

/** Switch language in place (keeps the current photo), updating the URL. */
export function setLang(l: Lang) {
  try {
    localStorage.setItem(PREF, l);
  } catch {
    /* storage unavailable */
  }
  if (l === current) return;
  current = l;
  history.pushState(null, "", langPath(l, BASE) + location.search + location.hash);
  applyStatic();
  window.dispatchEvent(new Event("langchange"));
}

window.addEventListener("popstate", () => {
  const l = fromPath() ?? "en";
  if (l !== current) {
    current = l;
    applyStatic();
    window.dispatchEvent(new Event("langchange"));
  }
});

/**
 * Apply translations to markup:
 * - data-i18n="key"        → textContent
 * - data-i18n-html="key"   → innerHTML (trusted strings from messages.ts)
 * - data-i18n-attr="attr:key,attr2:key2"
 */
export function applyStatic(root: ParentNode = document) {
  document.documentElement.lang = current;
  document.title = t("meta.title");
  document.querySelector('meta[name="description"]')?.setAttribute("content", t("meta.description"));
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => (el.textContent = t(el.dataset.i18n as Key)));
  root.querySelectorAll<HTMLElement>("[data-i18n-html]").forEach((el) => (el.innerHTML = t(el.dataset.i18nHtml as Key)));
  root.querySelectorAll<HTMLElement>("[data-i18n-attr]").forEach((el) => {
    for (const pair of el.dataset.i18nAttr!.split(",")) {
      const [attr, key] = pair.split(":");
      el.setAttribute(attr.trim(), t(key.trim() as Key));
    }
  });
  document.querySelectorAll<HTMLAnchorElement>("[data-lang]").forEach((a) => {
    a.classList.toggle("on", a.dataset.lang === current);
    a.setAttribute("aria-current", String(a.dataset.lang === current));
  });
}
