import basicSsl from "@vitejs/plugin-basic-ssl";
import { defineConfig } from "vite";
import { i18nPages } from "./i18n-plugin";

// HTTPS=1 serves over a self-signed certificate: phones only allow camera
// access on secure origins, and localhost doesn't apply on the LAN.
const https = !!process.env.HTTPS;

export default defineConfig({
  plugins: [i18nPages(), ...(https ? [basicSsl()] : [])],
  worker: { format: "es" },
  build: { target: "es2022" },
  server: { host: "0.0.0.0" },
  preview: { host: "0.0.0.0" },
});
