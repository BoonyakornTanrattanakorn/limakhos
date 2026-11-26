import type {
  AnalysisResult,
  GameOutcome,
  MoveDto,
  PositionSnapshot,
  WasmRequest,
  WasmResponse,
} from "./types";

interface PendingCall {
  resolve: (resp: WasmResponse) => void;
  reject: (err: Error) => void;
}

/**
 * Off-main-thread client for the local Rust WASM rules & analysis engine.
 */
export class MakhosEngineClient {
  private worker: Worker;
  private nextId = 1;
  private pending = new Map<number, PendingCall>();

  constructor() {
    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (ev: MessageEvent<{ id: number; resp: WasmResponse }>) => {
      const { id, resp } = ev.data;
      const entry = this.pending.get(id);
      if (!entry) return;
      this.pending.delete(id);
      if (resp.ok === "error") {
        entry.reject(new Error(resp.message));
      } else {
        entry.resolve(resp);
      }
    };
    this.worker.onerror = (ev) => {
      const err = new Error(ev.message || "WASM worker crashed");
      for (const [, p] of this.pending) {
        p.reject(err);
      }
      this.pending.clear();
    };
  }

  private call(req: WasmRequest): Promise<WasmResponse> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, req });
    });
  }

  async inspect(fen?: string, historyHashes?: string[]): Promise<PositionSnapshot> {
    const resp = await this.call({ cmd: "inspect", fen, history_hashes: historyHashes });
    if (resp.ok !== "snapshot") throw new Error(`Unexpected response: ${resp.ok}`);
    return resp;
  }

  async applyMove(
    fen: string,
    move: string,
    historyHashes?: string[],
  ): Promise<{ notation: string; applied: MoveDto; snapshot: PositionSnapshot }> {
    const resp = await this.call({
      cmd: "apply_move",
      fen,
      move,
      history_hashes: historyHashes,
    });
    if (resp.ok !== "move_applied") throw new Error(`Unexpected response: ${resp.ok}`);
    return { notation: resp.notation, applied: resp.applied, snapshot: resp.snapshot };
  }

  async analyze(
    fen: string,
    opts: { maxDepth?: number; maxNodes?: number; multipv?: number; historyHashes?: string[] } = {},
  ): Promise<AnalysisResult> {
    const resp = await this.call({
      cmd: "analyze",
      fen,
      max_depth: opts.maxDepth ?? 8,
      max_nodes: opts.maxNodes ?? 350_000,
      multipv: opts.multipv ?? 3,
      history_hashes: opts.historyHashes,
    });
    if (resp.ok !== "analysis") throw new Error(`Unexpected response: ${resp.ok}`);
    return resp;
  }

  async aiMove(
    fen: string,
    level: number,
    historyHashes?: string[],
  ): Promise<{ chosen: MoveDto | null; analysis: AnalysisResult }> {
    const seed = Math.floor(Math.random() * 1_000_000_000);
    const resp = await this.call({
      cmd: "ai_move",
      fen,
      level,
      seed,
      history_hashes: historyHashes,
    });
    if (resp.ok !== "ai_move") throw new Error(`Unexpected response: ${resp.ok}`);
    return { chosen: resp.chosen, analysis: resp.analysis };
  }

  async exportPdn(params: {
    initialFen?: string;
    moves: string[];
    event?: string;
    lightPlayer?: string;
    darkPlayer?: string;
  }): Promise<string> {
    const resp = await this.call({
      cmd: "export_pdn",
      initial_fen: params.initialFen,
      moves: params.moves,
      event: params.event,
      light_player: params.lightPlayer,
      dark_player: params.darkPlayer,
    });
    if (resp.ok !== "pdn") throw new Error(`Unexpected response: ${resp.ok}`);
    return resp.pdn;
  }

  async importPdn(pdn: string): Promise<{
    initialFen: string;
    notations: string[];
    fens: string[];
    hashes: string[];
    outcome: GameOutcome;
  }> {
    const resp = await this.call({ cmd: "import_pdn", pdn });
    if (resp.ok !== "imported_pdn") throw new Error(`Unexpected response: ${resp.ok}`);
    return {
      initialFen: resp.initial_fen,
      notations: resp.notations,
      fens: resp.fens,
      hashes: resp.hashes,
      outcome: resp.outcome,
    };
  }

  async clearCache(): Promise<void> {
    await this.call({ cmd: "clear_cache" });
  }

  destroy(): void {
    this.worker.terminate();
  }
}
