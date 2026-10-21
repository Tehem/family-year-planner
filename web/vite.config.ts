import { resolve } from "node:path";
import { defineConfig } from "vite";

// Multi-page build: every entry point is a static HTML page served by the Rust binary.
export default defineConfig({
  root: __dirname,
  base: "/",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    assetsInlineLimit: 0,
    rollupOptions: {
      input: {
        index: resolve(__dirname, "index.html"),
        connexion: resolve(__dirname, "connexion.html"),
        reglages: resolve(__dirname, "reglages.html"),
        kiosk: resolve(__dirname, "kiosk.html"),
        consent: resolve(__dirname, "consent.html"),
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/auth": "http://127.0.0.1:8080",
      "/oauth": "http://127.0.0.1:8080",
      "/kiosk": "http://127.0.0.1:8080",
      "/healthz": "http://127.0.0.1:8080",
    },
  },
});
