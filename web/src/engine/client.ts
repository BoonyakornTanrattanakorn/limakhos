import type {
  AnalysisLine,
  AnalysisProgress,
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
  onProgress?: (progress: AnalysisProgress) => void;
}

interface WorkerSlot {
  worker: Worker;
  pending: Map<number, PendingCall>;
  abortView: Int32Array | null;
  softAborting: boolean;
  drainResolvers: Array<() => void>;
}

interface WorkerOutbound {
  id: number;
  resp?: WasmResponse;
  progress?: AnalysisProgress;
}

export class AnalysisCancelledError extends Error {
  constructor() {
    super("Analysis cancelled");
    this.name = "AnalysisCancelledError";
  }
}

export interface AnalysisProgressUpdate {
  depth: number;
  nodes: number;
  lines: AnalysisLine[];
}

export interface AnalyzeOptions {
  maxDepth?: number;
  maxNodes?: number;
  maxTimeMs?: number;
  threads?: number;
  multipv?: number;
  historyHashes?: string[];
  legalMoves?: string[];
  resume?: boolean;
  onProgress?: (update: AnalysisProgressUpdate) => void;
}

/**
 * Off-main-thread client for the local Rust WASM rules & analysis engine.
 *
 * Uses a dedicated interactive worker for fast rules operations (`inspect`, `applyMove`,
 * PDN I/O, `aiMove`) so moving pieces and navigating history are NEVER blocked by
 * background analysis, plus a separate preemptible Web Worker pool for streaming
 * single- or multi-threaded analysis.
 */
export class MakhosEngineClient {
  private rulesSlot: WorkerSlot | null = null;
  private analysisSlots: WorkerSlot[] = [];
  private nextId = 1;
  private activeAnalysisRunId = 0;
  private compiledModule: WebAssembly.Module | null = null;
  private compilePromise: Promise<WebAssembly.Module | null> | null = null;

  constructor() {
    this.warmWasmModule();
    this.ensureRulesSlot();
  }

  private warmWasmModule(): void {
    if (this.compilePromise || typeof fetch !== "function" || typeof WebAssembly === "undefined") {
      return;
    }
    this.compilePromise = (async () => {
      try {
        const res = await fetch(`${import.meta.env.BASE_URL}wasm/makhos_wasm.wasm`);
        if (!res.ok) return null;
        const bytes = await res.arrayBuffer();
        const mod = await WebAssembly.compile(bytes);
        this.compiledModule = mod;
        return mod;
      } catch {
        return null;
      }
    })();
  }

  private canUseSharedAbort(): boolean {
    return (
      typeof SharedArrayBuffer !== "undefined" &&
      typeof Atomics !== "undefined" &&
      typeof globalThis !== "undefined" &&
      (globalThis as { crossOriginIsolated?: boolean }).crossOriginIsolated === true
    );
  }

  private flushDrain(slot: WorkerSlot): void {
    if (slot.pending.size > 0) return;
    slot.softAborting = false;
    if (slot.drainResolvers.length > 0) {
      const resolvers = slot.drainResolvers.splice(0);
      for (const resolve of resolvers) resolve();
    }
  }

  private createSlot(withAbortBuffer = false): WorkerSlot {
    const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    const pending = new Map<number, PendingCall>();
    let abortBuffer: SharedArrayBuffer | undefined;
    let abortView: Int32Array | null = null;
    if (withAbortBuffer && this.canUseSharedAbort()) {
      abortBuffer = new SharedArrayBuffer(4);
      abortView = new Int32Array(abortBuffer);
    }
    const slot: WorkerSlot = {
      worker,
      pending,
      abortView,
      softAborting: false,
      drainResolvers: [],
    };

    if (this.compiledModule || abortBuffer) {
      try {
        worker.postMessage({
          type: "init",
          module: this.compiledModule ?? undefined,
          abortBuffer,
        });
      } catch {
        // Fallback to worker fetch
      }
    }

    worker.onmessage = (ev: MessageEvent<WorkerOutbound>) => {
      const { id, resp, progress } = ev.data;
      const entry = pending.get(id);
      if (!entry) return;
      if (progress) {
        entry.onProgress?.(progress);
        return;
      }
      if (!resp) return;
      pending.delete(id);
      this.flushDrain(slot);
      if (resp.ok === "error") {
        entry.reject(new Error(resp.message));
      } else {
        entry.resolve(resp);
      }
    };

    worker.onerror = (ev) => {
      const err = new Error(ev.message || "WASM worker crashed");
      for (const [, p] of pending) {
        p.reject(err);
      }
      pending.clear();
      this.flushDrain(slot);
    };

    return slot;
  }

  private ensureRulesSlot(): WorkerSlot {
    if (!this.rulesSlot) {
      this.rulesSlot = this.createSlot(false);
    }
    return this.rulesSlot;
  }

  private ensureAnalysisSlots(count: number): WorkerSlot[] {
    while (this.analysisSlots.length < count) {
      this.analysisSlots.push(this.createSlot(true));
    }
    return this.analysisSlots.slice(0, count);
  }

  private async waitForSlotIdle(slot: WorkerSlot): Promise<WorkerSlot> {
    if (slot.pending.size === 0) return slot;
    await new Promise<void>((resolve) => {
      let done = false;
      const finish = () => {
        if (done) return;
        done = true;
        clearTimeout(timer);
        resolve();
      };
      const timer = setTimeout(finish, 150);
      slot.drainResolvers.push(finish);
    });
    if (slot.pending.size === 0) return slot;

    // Fallback: worker did not drain in time, replace it.
    slot.worker.terminate();
    const err = new AnalysisCancelledError();
    for (const [, p] of slot.pending) {
      p.reject(err);
    }
    slot.pending.clear();
    this.flushDrain(slot);
    const fresh = this.createSlot(true);
    const idx = this.analysisSlots.indexOf(slot);
    if (idx !== -1) {
      this.analysisSlots[idx] = fresh;
    }
    return fresh;
  }

  private callOnSlot(
    slot: WorkerSlot,
    req: WasmRequest,
    onProgress?: (progress: AnalysisProgress) => void,
  ): Promise<WasmResponse> {
    const id = this.nextId++;
    if (slot.abortView) {
      Atomics.store(slot.abortView, 0, 0);
    }
    return new Promise((resolve, reject) => {
      slot.pending.set(id, { resolve, reject, onProgress });
      slot.worker.postMessage({ id, req });
    });
  }

  /**
   * Cooperatively abort any running analysis workers so they exit cleanly
   * while preserving their transposition tables and `last_search` state.
   * Falls back to terminating busy workers when SharedArrayBuffer is unavailable.
   * Returns true if all running workers were stopped without termination.
   */
  stopAnalysisSoft(): boolean {
    this.activeAnalysisRunId += 1;
    let preservedAll = true;
    const survivors: WorkerSlot[] = [];
    for (const slot of this.analysisSlots) {
      if (slot.pending.size > 0) {
        if (slot.abortView) {
          slot.softAborting = true;
          Atomics.store(slot.abortView, 0, 1);
          survivors.push(slot);
        } else {
          preservedAll = false;
          slot.worker.terminate();
          const err = new AnalysisCancelledError();
          for (const [, p] of slot.pending) {
            p.reject(err);
          }
          slot.pending.clear();
          this.flushDrain(slot);
        }
      } else {
        survivors.push(slot);
      }
    }
    this.analysisSlots = survivors;
    return preservedAll;
  }

  /**
   * Immediately preempt any in-flight analysis workers without touching the
   * interactive rules worker or idle analysis workers.
   */
  cancelAnalysis(): void {
    this.stopAnalysisSoft();
  }

  async inspect(fen?: string, historyHashes?: string[]): Promise<PositionSnapshot> {
    const resp = await this.callOnSlot(this.ensureRulesSlot(), {
      cmd: "inspect",
      fen,
      history_hashes: historyHashes,
    });
    if (resp.ok !== "snapshot") throw new Error(`Unexpected response: ${resp.ok}`);
    return resp;
  }

  async applyMove(
    fen: string,
    move: string,
    historyHashes?: string[],
  ): Promise<{ notation: string; applied: MoveDto; snapshot: PositionSnapshot }> {
    const resp = await this.callOnSlot(this.ensureRulesSlot(), {
      cmd: "apply_move",
      fen,
      move,
      history_hashes: historyHashes,
    });
    if (resp.ok !== "move_applied") throw new Error(`Unexpected response: ${resp.ok}`);
    return { notation: resp.notation, applied: resp.applied, snapshot: resp.snapshot };
  }

  async analyze(fen: string, opts: AnalyzeOptions = {}): Promise<AnalysisResult> {
    this.cancelAnalysis();
    const runId = this.activeAnalysisRunId;

    const maxDepth = Math.max(1, Math.min(48, Math.round(opts.maxDepth ?? 8)));
    const maxTimeMs =
      opts.maxTimeMs !== undefined && opts.maxTimeMs > 0 ? Math.round(opts.maxTimeMs) : undefined;
    const maxNodes = opts.maxNodes;
    const multipv = Math.max(1, opts.multipv ?? 3);
    const threads = Math.max(1, Math.min(16, Math.round(opts.threads ?? 1)));

    let moveNotations = opts.legalMoves;
    if (threads > 1 && !moveNotations) {
      const snap = await this.inspect(fen, opts.historyHashes);
      if (runId !== this.activeAnalysisRunId) {
        throw new AnalysisCancelledError();
      }
      moveNotations = snap.legal_moves.map((m) => m.long_notation);
    }

    if (threads <= 1 || !moveNotations || moveNotations.length <= 1) {
      const [rawSlot] = this.ensureAnalysisSlots(1);
      const slot = await this.waitForSlotIdle(rawSlot);
      if (runId !== this.activeAnalysisRunId) {
        throw new AnalysisCancelledError();
      }
      let latestLines: AnalysisLine[] = [];
      let lastEmit = 0;

      const resp = await this.callOnSlot(
        slot,
        {
          cmd: "analyze",
          fen,
          max_depth: maxDepth,
          max_nodes: maxNodes,
          max_time_ms: maxTimeMs,
          multipv,
          history_hashes: opts.historyHashes,
          resume: opts.resume ?? false,
        },
        (p) => {
          if (runId !== this.activeAnalysisRunId) return;
          const hasLines = p.lines !== undefined;
          if (hasLines) {
            latestLines = p.lines!;
          }
          const now = performance.now();
          if (!hasLines && now - lastEmit < 50) return;
          lastEmit = now;
          opts.onProgress?.({
            depth: p.depth,
            nodes: p.nodes,
            lines: latestLines,
          });
        },
      );
      if (runId !== this.activeAnalysisRunId) {
        throw new AnalysisCancelledError();
      }
      if (resp.ok !== "analysis") throw new Error(`Unexpected response: ${resp.ok}`);
      return resp;
    }

    const workerCount = Math.min(threads, moveNotations.length);
    const buckets: string[][] = Array.from({ length: workerCount }, () => []);
    moveNotations.forEach((mv, idx) => {
      buckets[idx % workerCount].push(mv);
    });

    const perWorkerNodes =
      maxNodes !== undefined && maxNodes > 0
        ? Math.max(1, Math.ceil(maxNodes / workerCount))
        : maxNodes;

    const rawSlots = this.ensureAnalysisSlots(workerCount);
    const slots = await Promise.all(rawSlots.map((s) => this.waitForSlotIdle(s)));
    if (runId !== this.activeAnalysisRunId) {
      throw new AnalysisCancelledError();
    }
    const resume = opts.resume ?? false;
    const states = Array.from({ length: workerCount }, () => ({
      depth: 0,
      nodes: 0,
      lines: [] as AnalysisLine[],
      foundMate: false,
      initialized: !resume,
    }));

    let lastEmit = 0;
    const emitMerged = (force: boolean) => {
      if (runId !== this.activeAnalysisRunId || !opts.onProgress) return;
      if (states.some((s) => !s.initialized)) return;
      const now = performance.now();
      if (!force && now - lastEmit < 55) return;
      lastEmit = now;

      let totalNodes = 0;
      const nonMateDepths: number[] = [];
      let maxDepthSeen = 0;
      const allLines: AnalysisLine[] = [];

      for (const st of states) {
        totalNodes += st.nodes;
        if (st.depth > maxDepthSeen) maxDepthSeen = st.depth;
        if (!st.foundMate) {
          nonMateDepths.push(st.depth);
        }
        allLines.push(...st.lines);
      }

      const depth =
        nonMateDepths.length > 0 ? Math.min(...nonMateDepths) : maxDepthSeen;

      allLines.sort((a, b) => b.score_cp - a.score_cp);
      const mergedLines = allLines.slice(0, multipv).map((line, i) => ({
        ...line,
        multipv: i + 1,
      }));

      opts.onProgress({
        depth,
        nodes: totalNodes,
        lines: mergedLines,
      });
    };

    const results = await Promise.all(
      buckets.map(async (rootMoves, idx) => {
        const resp = await this.callOnSlot(
          slots[idx],
          {
            cmd: "analyze",
            fen,
            max_depth: maxDepth,
            max_nodes: perWorkerNodes,
            max_time_ms: maxTimeMs,
            multipv: Math.min(multipv, rootMoves.length),
            history_hashes: opts.historyHashes,
            root_moves: rootMoves,
            resume,
          },
          (p) => {
            if (runId !== this.activeAnalysisRunId) return;
            states[idx].depth = p.depth;
            states[idx].nodes = p.nodes;
            if (p.lines !== undefined) {
              states[idx].lines = p.lines;
              states[idx].foundMate =
                p.lines.length > 0 && p.lines[0].mate_in !== null;
              states[idx].initialized = true;
            }
            emitMerged(false);
          },
        );
        if (resp.ok !== "analysis") throw new Error(`Unexpected response: ${resp.ok}`);
        states[idx].depth = resp.depth;
        states[idx].nodes = resp.nodes;
        states[idx].lines = resp.lines;
        states[idx].foundMate =
          resp.lines.length > 0 && resp.lines[0].mate_in !== null;
        states[idx].initialized = true;
        emitMerged(true);
        return resp;
      }),
    );

    if (runId !== this.activeAnalysisRunId) {
      throw new AnalysisCancelledError();
    }

    let totalNodes = 0;
    const nonMateDepths: number[] = [];
    let maxDepthSeen = 0;
    const allLines: AnalysisLine[] = [];
    for (const r of results) {
      totalNodes += r.nodes;
      if (r.depth > maxDepthSeen) maxDepthSeen = r.depth;
      const isMate = r.lines.length > 0 && r.lines[0].mate_in !== null;
      if (r.depth > 0 && !isMate) {
        nonMateDepths.push(r.depth);
      }
      allLines.push(...r.lines);
    }

    allLines.sort((a, b) => b.score_cp - a.score_cp);
    const mergedLines = allLines.slice(0, multipv).map((line, i) => ({
      ...line,
      multipv: i + 1,
    }));

    return {
      depth: nonMateDepths.length > 0 ? Math.min(...nonMateDepths) : maxDepthSeen,
      nodes: totalNodes,
      lines: mergedLines,
    };
  }

  async aiMove(
    fen: string,
    level: number,
    historyHashes?: string[],
  ): Promise<{ chosen: MoveDto | null; analysis: AnalysisResult }> {
    const seed = Math.floor(Math.random() * 1_000_000_000);
    const resp = await this.callOnSlot(this.ensureRulesSlot(), {
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
    const resp = await this.callOnSlot(this.ensureRulesSlot(), {
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
    const resp = await this.callOnSlot(this.ensureRulesSlot(), { cmd: "import_pdn", pdn });
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
    this.cancelAnalysis();
    const analysisTargets = await Promise.all(
      this.analysisSlots.map((slot) => this.waitForSlotIdle(slot)),
    );
    const targets: WorkerSlot[] = [...analysisTargets];
    if (this.rulesSlot) targets.push(this.rulesSlot);
    await Promise.all(
      targets.map((slot) => this.callOnSlot(slot, { cmd: "clear_cache" }).catch(() => {})),
    );
  }

  destroy(): void {
    this.cancelAnalysis();
    if (this.rulesSlot) {
      this.rulesSlot.worker.terminate();
      this.rulesSlot.pending.clear();
      this.rulesSlot = null;
    }
    for (const slot of this.analysisSlots) {
      slot.worker.terminate();
      slot.pending.clear();
    }
    this.analysisSlots = [];
  }
}
