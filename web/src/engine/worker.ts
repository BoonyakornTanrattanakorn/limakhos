/// <reference lib="webworker" />

interface WasmExports {
  memory: WebAssembly.Memory;
  wasm_alloc: (len: number) => number;
  wasm_dealloc: (ptr: number, len: number) => void;
  wasm_dispatch: (ptr: number, len: number) => bigint;
}

type WorkerInbound =
  | { type: "init"; module: WebAssembly.Module }
  | { id: number; req: unknown };

const encoder = new TextEncoder();
const decoder = new TextDecoder();

let precompiledModule: WebAssembly.Module | null = null;
let wasmExports: WasmExports | null = null;
let exportsPromise: Promise<WasmExports> | null = null;
let activeRequestId: number | null = null;

function buildImports(): WebAssembly.Imports {
  return {
    env: {
      host_now_ms: () => performance.now(),
      host_on_progress: (ptr: number, len: number) => {
        if (activeRequestId === null || !wasmExports) return;
        try {
          const bytes = new Uint8Array(wasmExports.memory.buffer, ptr, len).slice();
          const parsed = JSON.parse(decoder.decode(bytes));
          self.postMessage({ id: activeRequestId, progress: parsed });
        } catch {
          // Ignore malformed progress ticks
        }
      },
    },
  };
}

async function loadWasm(): Promise<WasmExports> {
  if (!exportsPromise) {
    exportsPromise = (async () => {
      const imports = buildImports();
      let instance: WebAssembly.Instance;
      try {
        if (precompiledModule) {
          instance = await WebAssembly.instantiate(precompiledModule, imports);
        } else {
          const url = `${import.meta.env.BASE_URL}wasm/makhos_wasm.wasm`;
          const res = await fetch(url);
          if (!res.ok) {
            throw new Error(`Failed to fetch ${url} (HTTP ${res.status})`);
          }
          const bytes = await res.arrayBuffer();
          const instantiated = await WebAssembly.instantiate(bytes, imports);
          instance = instantiated.instance;
        }
      } catch (err) {
        exportsPromise = null;
        throw err;
      }
      wasmExports = instance.exports as unknown as WasmExports;
      return wasmExports;
    })();
  }
  return exportsPromise;
}

function callDispatch(wasm: WasmExports, payload: string): string {
  const reqBytes = encoder.encode(payload);
  const reqPtr = wasm.wasm_alloc(reqBytes.length);
  new Uint8Array(wasm.memory.buffer, reqPtr, reqBytes.length).set(reqBytes);

  const packed = wasm.wasm_dispatch(reqPtr, reqBytes.length);
  wasm.wasm_dealloc(reqPtr, reqBytes.length);

  const respPtr = Number(packed & 0xffff_ffffn);
  const respLen = Number((packed >> 32n) & 0xffff_ffffn);
  const respBytes = new Uint8Array(wasm.memory.buffer, respPtr, respLen).slice();
  wasm.wasm_dealloc(respPtr, respLen);

  return decoder.decode(respBytes);
}

self.onmessage = async (ev: MessageEvent<WorkerInbound>) => {
  const data = ev.data;
  if ("type" in data) {
    precompiledModule = data.module;
    void loadWasm().catch(() => {});
    return;
  }

  const { id, req } = data;
  try {
    const wasm = await loadWasm();
    activeRequestId = id;
    let raw: string;
    try {
      raw = callDispatch(wasm, JSON.stringify(req));
    } finally {
      activeRequestId = null;
    }
    const parsed = JSON.parse(raw);
    self.postMessage({ id, resp: parsed });
  } catch (err) {
    activeRequestId = null;
    const message = err instanceof Error ? err.message : String(err);
    self.postMessage({ id, resp: { ok: "error", message } });
  }
};
