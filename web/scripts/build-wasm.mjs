import { execSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");
const wasmSrc = resolve(repoRoot, "target", "wasm32-unknown-unknown", "release", "makhos_wasm.wasm");
const wasmDstDir = resolve(repoRoot, "web", "public", "wasm");
const wasmDst = resolve(wasmDstDir, "makhos_wasm.wasm");

if (!process.env.SKIP_WASM_BUILD) {
  console.log("[limakhos] Building makhos-wasm (wasm32-unknown-unknown --release)...");
  execSync("cargo build -p makhos-wasm --target wasm32-unknown-unknown --release", {
    cwd: repoRoot,
    stdio: "inherit",
  });
}

if (!existsSync(wasmSrc)) {
  throw new Error(`Expected WASM artifact not found at ${wasmSrc}`);
}

mkdirSync(wasmDstDir, { recursive: true });
copyFileSync(wasmSrc, wasmDst);
console.log(`[limakhos] Copied WASM artifact -> ${wasmDst}`);
