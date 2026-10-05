// Build step: emit /fr/index.html, /de/index.html, /es/index.html with the
// static markup already translated (no flash of English, crawlable), plus
// <html lang>, title, description, canonical and hreflang alternates.
import fs from "node:fs";
import path from "node:path";
import type { Plugin } from "vite";
import { LANGS, MESSAGES, langPath, type Key, type Lang } from "./src/i18n/messages";

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

export function translateHtml(html: string, lang: Lang, site: string): string {
  const m = MESSAGES[lang];
  const tr = (k: string) => m[k as Key] ?? MESSAGES.en[k as Key] ?? k;
  let out = html.replace(/<html lang="[^"]*"/, `<html lang="${lang}"`);
  out = out.replace(/<title>[\s\S]*?<\/title>/, `<title>${esc(tr("meta.title"))}</title>`);
  out = out.replace(/(<meta name="description" content=")[^"]*(")/, `$1${esc(tr("meta.description"))}$2`);
  // Elements whose content is a translation (they contain no nested element of the same tag).
  out = out.replace(
    /(<(\w+)\b[^>]*?\sdata-i18n(-html)?="([^"]+)"[^>]*>)([\s\S]*?)(<\/\2>)/g,
    (_all, open, _tag, isHtml, key, _inner, close) => open + (isHtml ? tr(key) : esc(tr(key))) + close,
  );
  // Attributes: data-i18n-attr="attr:key,attr2:key2".
  out = out.replace(/<[^>]*\sdata-i18n-attr="([^"]+)"[^>]*>/g, (tag, spec: string) => {
    for (const pair of spec.split(",")) {
      const [attr, key] = pair.split(":").map((x) => x.trim());
      tag = tag.replace(new RegExp(`(\\s${attr}=")[^"]*(")`), `$1${esc(tr(key))}$2`);
    }
    return tag;
  });
  // Mark the current language in the footer.
  out = out.replace(new RegExp(`(<a [^>]*data-lang="${lang}")`), `$1 class="on" aria-current="true"`);
  const alternates = LANGS.map((l) => `<link rel="alternate" hreflang="${l}" href="${site}${langPath(l)}" />`).join("\n    ");
  const head =
    `<link rel="canonical" href="${site}${langPath(lang)}" />\n    ${alternates}\n    ` +
    `<link rel="alternate" hreflang="x-default" href="${site}/" />`;
  return out.replace("<!-- hreflang alternates are added at build time -->", head);
}

export function i18nPages(): Plugin {
  let outDir = "dist";
  return {
    name: "i18n-pages",
    apply: "build",
    configResolved(c) {
      outDir = path.resolve(c.root, c.build.outDir);
    },
    closeBundle() {
      // Absolute URLs for canonical/hreflang; set SITE_URL when deploying.
      const site = (process.env.SITE_URL ?? "").replace(/\/$/, "");
      const base = fs.readFileSync(path.join(outDir, "index.html"), "utf8");
      for (const lang of LANGS) {
        const file = lang === "en" ? path.join(outDir, "index.html") : path.join(outDir, lang, "index.html");
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(file, translateHtml(base, lang, site));
      }
    },
  };
}
