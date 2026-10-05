# Deployment guide

The web app is a fully static site (`web/dist`), so hosting is just file serving. This guide is the
plan to follow when you're ready to launch.

## Recommended host: Cloudflare Pages (free)

Why:

- **Unmetered bandwidth** on static files. Each first visit downloads ~20 MB (≈4 MB gzipped wasm +
  ≈16 MB models that barely compress); bandwidth-capped hosts get expensive or hit limits quickly
  (e.g. 100 GB/month ≈ 5,000 new visitors).
- **25 MiB per-file limit** — our largest files are the wasm (≈16.4 MB) and MODNet (≈13 MB).
- `_headers` file support (CSP etc.), automatic HTTPS, HTTP/3, Brotli, preview deployments per branch.
- Cost: €0 hosting + the domain (~€10–15/year).

Alternatives: Cloudflare Workers with static assets (same network; choose it if you later want
server-side code), Netlify (supports `_headers`, but bandwidth-capped free tier), GitHub Pages (no
custom headers → no CSP; not recommended).

## Step by step

### 1. Domain

- Buy it through **Cloudflare Registrar** (at-cost pricing) if the TLD is supported, or buy elsewhere and
  point the domain's nameservers to Cloudflare.

### 2. First deploy by hand (to check everything)

```sh
cd web
npm run wasm
SITE_URL=https://your-domain.example npm run build
npx wrangler login                                  # opens the browser once
npx wrangler pages project create auto-photo --production-branch main
npx wrangler pages deploy dist --project-name auto-photo
```

This gives a `https://auto-photo.pages.dev` URL. Check: start page, `/fr/`, camera on a phone, a full
photo run, downloads, and the browser console (no CSP errors).

### 3. Custom domain

Cloudflare dashboard → Workers & Pages → `auto-photo` → Custom domains → add `your-domain.example`
(and `www.` if wanted; redirect one to the other with a Redirect Rule).

`SITE_URL` must be the final domain: it's used for `<link rel="canonical">` and hreflang alternates
in the pre-rendered pages.

### 4. Automatic deploys (GitHub Actions)

Build in CI rather than with Cloudflare's builder (it doesn't ship the Rust/wasm toolchain).

1. Push the repo to GitHub.
2. Create a Cloudflare API token with the *Cloudflare Pages — Edit* permission; add repository secrets
   `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`, and a repository variable `SITE_URL`.
3. Save as `.github/workflows/deploy.yml`:

```yaml
name: deploy
on:
  push:
    branches: [main]
  pull_request:

jobs:
  deploy:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      deployments: write
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - uses: Swatinem/rust-cache@v2
      - name: wasm-bindgen (must match Cargo.lock)
        run: cargo install wasm-bindgen-cli --version 0.2.129 --locked

      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
          cache-dependency-path: web/package-lock.json
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"

      - name: Cache models
        uses: actions/cache@v4
        with:
          path: models/src
          key: models-${{ hashFiles('scripts/fetch_models.sh') }}
      - run: ./scripts/fetch_models.sh

      - name: Tests
        run: cargo test --workspace --exclude photo-wasm

      - name: Build site
        working-directory: web
        env:
          SITE_URL: ${{ vars.SITE_URL }}
        run: |
          npm ci
          npm run wasm
          npm run build

      - name: Deploy to Cloudflare Pages
        uses: cloudflare/wrangler-action@v3
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          accountId: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
          command: pages deploy web/dist --project-name=auto-photo --branch=${{ github.head_ref || github.ref_name }}
```

Pushes to `main` deploy to production; pull requests get a preview URL. (Optionally add
`sudo apt-get install -y binaryen` before the build to enable `wasm-opt`; check the apt version supports
`--enable-simd`.)

## What's already production-ready

- `npm run build` → `web/dist`: `index.html` (English), `fr/`, `de/`, `es/` pre-rendered with translated
  content, `<html lang>`, title, description, canonical and hreflang; hashed assets; `models/`.
- `web/public/_headers` (copied into `dist/`):
  - CSP with `connect-src 'self'`: the browser refuses any request to another origin, which backs up the
    "nothing is uploaded" promise. `'wasm-unsafe-eval'` is required for WebAssembly.
  - `Permissions-Policy` (camera on this origin only), `Referrer-Policy: no-referrer`, `nosniff`, COOP.
  - Immutable caching for `/assets/*`, 30 days for `/models/*`.

## Pre-launch checklist

- [ ] **Domain** bought; `SITE_URL` set.
- [ ] **wasm-opt**: `brew install binaryen` (or in CI) and rebuild — expect 10–20% smaller wasm.
- [ ] **Versioned models**: move to `/models/v1/` (worker `fetchModel` URL + `build-wasm.sh` + `_headers`)
      and mark them `immutable`; bump `MODEL_CACHE` in `web/src/worker.ts` when models change.
- [ ] **Offline/PWA**: service worker precaching the app shell and wasm; `manifest.webmanifest` + icons.
- [ ] **Third-party notices**: publish the Apache-2.0/MIT/OFL licence texts (e.g. `/licenses.txt`) and
      link them from the footer credits; choose a licence for your own code.
- [ ] **Legal**: if published from France, the LCEN requires *mentions légales* (publisher identity —
      non-professional publishers may stay anonymous by giving their details to the host — and the host:
      Cloudflare, Inc. — take the current legal address from cloudflare.com). Add them to the footer's legal section.
      No cookies and no analytics ⇒ no consent banner needed.
- [ ] **SEO/social**: `robots.txt`, `sitemap.xml` (with hreflang), Open Graph/Twitter image and tags.
- [ ] **Device testing**: iOS Safari ≥ 16.4 (wasm SIMD), mid-range Android Chrome, memory on low-end
      phones (images are capped at 2400 px on the long side).
- [ ] **Translations** reviewed by native speakers (especially the legal text).
- [ ] Optional **analytics**: Cloudflare Web Analytics is cookieless, but needs the CSP extended
      (`script-src https://static.cloudflareinsights.com; connect-src 'self' https://cloudflareinsights.com`)
      — weigh against the "nothing leaves your device" message.

## Limits to keep in mind

- Cloudflare Pages: 25 MiB per file, 20,000 files per site (free), 100 `_headers` rules.
- Model and wasm sizes are the only large files; keep each under 25 MiB if models change.
