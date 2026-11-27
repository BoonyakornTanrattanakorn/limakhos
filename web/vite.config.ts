import { defineConfig } from "vite";

const ISOLATION_HEADERS = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "credentialless",
};

export default defineConfig({
  base: process.env.VITE_BASE_PATH ?? "/",
  server: {
    port: 5173,
    headers: ISOLATION_HEADERS,
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/ws": {
        target: "ws://127.0.0.1:8080",
        ws: true,
      },
    },
  },
  preview: {
    headers: ISOLATION_HEADERS,
  },
  build: {
    outDir: "dist",
    sourcemap: false,
  },
});
