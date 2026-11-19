// Build step: emit /fr/index.html, /de/index.html, /es/index.html with the
// static markup already translated (no flash of English, crawlable), plus
// <html lang>, title, description, canonical, hreflang alternates, Open Graph /
// Twitter cards, JSON-LD, a CSP meta tag, robots.txt and sitemap.xml.
import fs from "node:fs";
import path from "node:path";
import type { Plugin } from "vite";
import { LANGS, MESSAGES, langPath, type Key, type Lang } from "./src/i18n/messages";

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

const OG_LOCALE: Record<Lang, string> = { en: "en_GB", fr: "fr_FR", de: "de_DE", es: "es_ES" };

// Same policy as public/_headers, for hosts that can't send headers (GitHub Pages).
// frame-ancestors is not allowed in a meta CSP, so it is left out here.
const CSP =
  "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; " +
  "img-src 'self' data: blob:; media-src 'self' blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; " +
  "object-src 'none'; base-uri 'self'; form-action 'none'";

/**
 * `site`: absolute public URL of the English page without trailing slash (SITE_URL), or "" if unknown.
 * `base`: public path the site is served from, ending with "/" (Vite `base`).
 */
export function translateHtml(html: string, lang: Lang, site: string, base = "/"): string {
  const m = MESSAGES[lang];
  const tr = (k: string) => m[k as Key] ?? MESSAGES.en[k as Key] ?? k;
  const title = tr("meta.title");
  const description = tr("meta.description");
  let out = html.replace(/<html lang="[^"]*"/, `<html lang="${lang}"`);
  out = out.replace(/<title>[\s\S]*?<\/title>/, `<title>${esc(title)}</title>`);
  out = out.replace(/(<meta name="description" content=")[^"]*(")/, `$1${esc(description)}$2`);
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
  // Footer language links follow the deployment base path.
  out = out.replace(/<a href="[^"]*"( data-lang="(\w+)")/g, (_all, attr, l: Lang) => `<a href="${langPath(l, base)}"${attr}`);
  // Mark the current language in the footer.
  out = out.replace(new RegExp(`(<a [^>]*data-lang="${lang}")`), `$1 class="on" aria-current="true"`);

  const head: string[] = [
    `<meta http-equiv="Content-Security-Policy" content="${CSP}" />`,
    `<meta name="referrer" content="no-referrer" />`,
    `<meta name="robots" content="index, follow" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:site_name" content="auto·photo" />`,
    `<meta property="og:title" content="${esc(title)}" />`,
    `<meta property="og:description" content="${esc(description)}" />`,
    `<meta property="og:locale" content="${OG_LOCALE[lang]}" />`,
    ...LANGS.filter((l) => l !== lang).map((l) => `<meta property="og:locale:alternate" content="${OG_LOCALE[l]}" />`),
    `<meta name="twitter:card" content="summary_large_image" />`,
    `<meta name="twitter:title" content="${esc(title)}" />`,
    `<meta name="twitter:description" content="${esc(description)}" />`,
  ];
  if (site) {
    const url = `${site}${langPath(lang)}`;
    head.push(
      `<link rel="canonical" href="${url}" />`,
      ...LANGS.map((l) => `<link rel="alternate" hreflang="${l}" href="${site}${langPath(l)}" />`),
      `<link rel="alternate" hreflang="x-default" href="${site}/" />`,
      `<meta property="og:url" content="${url}" />`,
      `<meta property="og:image" content="${site}/og.png" />`,
      `<meta property="og:image:width" content="1200" />`,
      `<meta property="og:image:height" content="630" />`,
      `<meta name="twitter:image" content="${site}/og.png" />`,
    );
    const ld = {
      "@context": "https://schema.org",
      "@type": "WebApplication",
      name: "auto·photo",
      url,
      description,
      inLanguage: lang,
      applicationCategory: "PhotographyApplication",
      operatingSystem: "Any (modern web browser)",
      browserRequirements: "Requires JavaScript and WebAssembly",
      isAccessibleForFree: true,
      offers: { "@type": "Offer", price: "0", priceCurrency: "EUR" },
      image: `${site}/og.png`,
    };
    head.push(`<script type="application/ld+json">${JSON.stringify(ld).replace(/</g, "\\u003c")}</script>`);
  }
  return out.replace("<!-- hreflang alternates are added at build time -->", head.join("\n    "));
}

function sitemap(site: string): string {
  const alts = LANGS.map((l) => `    <xhtml:link rel="alternate" hreflang="${l}" href="${site}${langPath(l)}"/>`).join("\n");
  const urls = LANGS.map((l) => `  <url>\n    <loc>${site}${langPath(l)}</loc>\n${alts}\n  </url>`).join("\n");
  return (
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">\n${urls}\n</urlset>\n`
  );
}

export function i18nPages(): Plugin {
  let outDir = "dist";
  let base = "/";
  return {
    name: "i18n-pages",
    apply: "build",
    configResolved(c) {
      outDir = path.resolve(c.root, c.build.outDir);
      base = c.base.endsWith("/") ? c.base : c.base + "/";
    },
    closeBundle() {
      // Absolute URL of the English page for canonical/hreflang/OG; set SITE_URL when deploying.
      const site = (process.env.SITE_URL ?? "").replace(/\/$/, "");
      const html = fs.readFileSync(path.join(outDir, "index.html"), "utf8");
      for (const lang of LANGS) {
        const file = lang === "en" ? path.join(outDir, "index.html") : path.join(outDir, lang, "index.html");
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(file, translateHtml(html, lang, site, base));
      }
      if (site) {
        fs.writeFileSync(path.join(outDir, "sitemap.xml"), sitemap(site));
        fs.writeFileSync(path.join(outDir, "robots.txt"), `User-agent: *\nAllow: /\n\nSitemap: ${site}/sitemap.xml\n`);
      }
    },
  };
}
