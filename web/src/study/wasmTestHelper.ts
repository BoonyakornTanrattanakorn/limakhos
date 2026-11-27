import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { AnalysisProgress, WasmRequest, WasmResponse } from "../engine/types";

interface WasmExports {
  memory: WebAssembly.Memory;
  wasm_alloc: (len: number) => number;
  wasm_dealloc: (ptr: number, len: number) => void;
  wasm_dispatch: (ptr: number, len: number) => bigint;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

let cachedExports: WasmExports | null = null;
let activeProgressHandler: ((p: AnalysisProgress) => void) | null = null;

export async function loadNodeWasm(): Promise<WasmExports> {
  if (cachedExports) return cachedExports;
  const wasmPath = resolve(__dirname, "..", "..", "public", "wasm", "makhos_wasm.wasm");
  const bytes = readFileSync(wasmPath);
  const instantiated = await WebAssembly.instantiate(bytes, {
    env: {
      host_now_ms: () => performance.now(),
      host_should_abort: () => 0,
      host_on_progress: (ptr: number, len: number) => {
        if (!activeProgressHandler || !cachedExports) return;
        const slice = new Uint8Array(cachedExports.memory.buffer, ptr, len).slice();
        activeProgressHandler(JSON.parse(decoder.decode(slice)) as AnalysisProgress);
      },
    },
  });
  cachedExports = instantiated.instance.exports as unknown as WasmExports;
  return cachedExports;
}

export async function callWasm(
  req: WasmRequest,
  onProgress?: (p: AnalysisProgress) => void,
): Promise<WasmResponse> {
  const wasm = await loadNodeWasm();
  const payload = JSON.stringify(req);
  const reqBytes = encoder.encode(payload);
  const reqPtr = wasm.wasm_alloc(reqBytes.length);
  new Uint8Array(wasm.memory.buffer, reqPtr, reqBytes.length).set(reqBytes);

  activeProgressHandler = onProgress ?? null;
  let packed: bigint;
  try {
    packed = wasm.wasm_dispatch(reqPtr, reqBytes.length);
  } finally {
    activeProgressHandler = null;
  }
  wasm.wasm_dealloc(reqPtr, reqBytes.length);

  const respPtr = Number(packed & 0xffff_ffffn);
  const respLen = Number((packed >> 32n) & 0xffff_ffffn);
  const respBytes = new Uint8Array(wasm.memory.buffer, respPtr, respLen).slice();
  wasm.wasm_dealloc(respPtr, respLen);

  return JSON.parse(decoder.decode(respBytes)) as WasmResponse;
}
