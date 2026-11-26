/// <reference lib="webworker" />

interface WasmExports {
  memory: WebAssembly.Memory;
  wasm_alloc: (len: number) => number;
  wasm_dealloc: (ptr: number, len: number) => void;
  wasm_dispatch: (ptr: number, len: number) => bigint;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

let exportsPromise: Promise<WasmExports> | null = null;

async function loadWasm(): Promise<WasmExports> {
  if (!exportsPromise) {
    exportsPromise = (async () => {
      const url = "/wasm/makhos_wasm.wasm";
      let instance: WebAssembly.Instance;
      try {
        const res = await fetch(url);
        if (!res.ok) {
          throw new Error(`Failed to fetch ${url} (HTTP ${res.status})`);
        }
        const bytes = await res.arrayBuffer();
        const instantiated = await WebAssembly.instantiate(bytes, {});
        instance = instantiated.instance;
      } catch (err) {
        exportsPromise = null;
        throw err;
      }
      return instance.exports as unknown as WasmExports;
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

self.onmessage = async (ev: MessageEvent<{ id: number; req: unknown }>) => {
  const { id, req } = ev.data;
  try {
    const wasm = await loadWasm();
    const raw = callDispatch(wasm, JSON.stringify(req));
    const parsed = JSON.parse(raw);
    self.postMessage({ id, resp: parsed });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    self.postMessage({ id, resp: { ok: "error", message } });
  }
};
