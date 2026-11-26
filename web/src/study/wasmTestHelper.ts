import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import type { WasmRequest, WasmResponse } from "../engine/types";

interface WasmExports {
  memory: WebAssembly.Memory;
  wasm_alloc: (len: number) => number;
  wasm_dealloc: (ptr: number, len: number) => void;
  wasm_dispatch: (ptr: number, len: number) => bigint;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

let cachedExports: WasmExports | null = null;

export async function loadNodeWasm(): Promise<WasmExports> {
  if (cachedExports) return cachedExports;
  const wasmPath = resolve(__dirname, "..", "..", "public", "wasm", "makhos_wasm.wasm");
  const bytes = readFileSync(wasmPath);
  const instantiated = await WebAssembly.instantiate(bytes, {});
  cachedExports = instantiated.instance.exports as unknown as WasmExports;
  return cachedExports;
}

export async function callWasm(req: WasmRequest): Promise<WasmResponse> {
  const wasm = await loadNodeWasm();
  const payload = JSON.stringify(req);
  const reqBytes = encoder.encode(payload);
  const reqPtr = wasm.wasm_alloc(reqBytes.length);
  new Uint8Array(wasm.memory.buffer, reqPtr, reqBytes.length).set(reqBytes);

  const packed = wasm.wasm_dispatch(reqPtr, reqBytes.length);
  wasm.wasm_dealloc(reqPtr, reqBytes.length);

  const respPtr = Number(packed & 0xffff_ffffn);
  const respLen = Number((packed >> 32n) & 0xffff_ffffn);
  const respBytes = new Uint8Array(wasm.memory.buffer, respPtr, respLen).slice();
  wasm.wasm_dealloc(respPtr, respLen);

  return JSON.parse(decoder.decode(respBytes)) as WasmResponse;
}
