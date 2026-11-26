import "./styles/app.css";
import { MakhosBoard, type BoardArrow } from "./board/MakhosBoard";
import { MakhosEngineClient } from "./engine/client";
import type { AnalysisResult, Color, GameOutcome, MoveDto, PositionSnapshot } from "./engine/types";
import { MESSAGES, type Locale } from "./i18n/messages";
import { evalToLightPercent, formatClockMs, formatEvalLight } from "./ui/eval";

type PlayMode = "analysis" | "ai" | "local";

interface TimeControlOption {
  id: string;
  label: string;
  initialMs: number;
  incrementMs: number;
}

const TIME_CONTROLS: TimeControlOption[] = [
  { id: "unlimited", label: "∞", initialMs: 0, incrementMs: 0 },
  { id: "12+5", label: "12+5 (Standard)", initialMs: 12 * 60_000, incrementMs: 5_000 },
  { id: "5+3", label: "5+3 (Rapid)", initialMs: 5 * 60_000, incrementMs: 3_000 },
  { id: "3+2", label: "3+2 (Blitz)", initialMs: 3 * 60_000, incrementMs: 2_000 },
];

interface HistoryEntry {
  fen: string;
  hash: string;
  notation: string | null;
  lastMove: { from: number; to: number; path?: number[] } | null;
}

class LimakhosApp {
  private engine = new MakhosEngineClient();
  private board!: MakhosBoard;

  private locale: Locale = "th";
  private mode: PlayMode = "analysis";
  private humanColor: Color = "light";
  private aiLevel = 5;
  private timeControlId = "unlimited";
  private orientation: Color = "light";
  private showNumbers = true;
  private showMainDiagonal = true;

  private analysisEnabled = true;
  private analysisDepth = 8;
  private analysisMultiPv = 3;
  private analysisSeq = 0;
  private analysisBusy = false;
  private lastAnalysis: AnalysisResult | null = null;
  private lastAnalysisMs = 0;
  private aiThinking = false;

  private timeline: HistoryEntry[] = [];
  private cursor = 0;
  private currentSnapshot: PositionSnapshot | null = null;
  private forcedOutcome: GameOutcome | null = null;

  private clocksMs: Record<Color, number> = { light: 0, dark: 0 };
  private clockTimer: number | null = null;
  private lastTick = 0;
  private toastMessage = "";

  // DOM refs
  private root: HTMLElement;
  private boardMount!: HTMLElement;
  private evalFill!: HTMLElement;
  private evalTopLabel!: HTMLElement;
  private evalBottomLabel!: HTMLElement;
  private topPlayerEl!: HTMLElement;
  private bottomPlayerEl!: HTMLElement;
  private statusEl!: HTMLElement;
  private analysisPanelEl!: HTMLElement;
  private movesPanelEl!: HTMLElement;
  private ioTextarea!: HTMLTextAreaElement;
  private toastEl!: HTMLElement;

  constructor(root: HTMLElement) {
    this.root = root;
    this.renderShell();
    window.addEventListener("keydown", (ev) => this.handleKeydown(ev));
    void this.startNewGame();
  }

  private t() {
    return MESSAGES[this.locale];
  }

  private renderShell(): void {
    const t = this.t();
    this.root.innerHTML = `
      <header class="topbar">
        <div class="brand">
          <h1>${t.appTitle}</h1>
          <span class="subtitle">${t.subtitle}</span>
        </div>
        <div class="topbar-controls">
          <div class="tab-group" role="tablist">
            <button class="tab-btn ${this.mode === "analysis" ? "is-active" : ""}" data-mode="analysis">${t.modeAnalysis}</button>
            <button class="tab-btn ${this.mode === "ai" ? "is-active" : ""}" data-mode="ai">${t.modeAi}</button>
            <button class="tab-btn ${this.mode === "local" ? "is-active" : ""}" data-mode="local">${t.modeLocal}</button>
          </div>
          <button class="btn" id="btn-locale">${this.locale === "th" ? "EN" : "ไทย"}</button>
        </div>
      </header>

      <main class="workspace">
        <section class="board-column">
          <div class="player-bar" id="top-player"></div>
          <div class="board-stage">
            <div class="eval-bar" title="Evaluation (Light vs Dark)">
              <span class="eval-bar-label is-top" id="eval-top">0.0</span>
              <div class="eval-bar-fill" id="eval-fill" style="height: 50%"></div>
              <span class="eval-bar-label is-bottom" id="eval-bottom"></span>
            </div>
            <div id="board-mount"></div>
          </div>
          <div class="player-bar" id="bottom-player"></div>
          <div class="board-toolbar">
            <div class="toolbar-group">
              <button class="btn is-primary" id="btn-new">${t.newGame}</button>
              <button class="btn" id="btn-flip">${t.flipBoard}</button>
              <button class="btn ${this.showNumbers ? "is-active" : ""}" id="btn-nums">${t.numbersToggle}</button>
              <button class="btn ${this.showMainDiagonal ? "is-active" : ""}" id="btn-diag">${t.mainDiagToggle}</button>
            </div>
            <div class="toolbar-group">
              <button class="btn" id="btn-undo">${t.undo}</button>
              <button class="btn is-danger" id="btn-resign">${t.resign}</button>
            </div>
          </div>
        </section>

        <aside class="side-column">
          <div id="status-box"></div>

          ${
            this.mode !== "analysis"
              ? `
          <div class="card">
            <div class="settings-grid">
              ${
                this.mode === "ai"
                  ? `
              <label class="field">
                <span>${t.playAs}</span>
                <select id="sel-side">
                  <option value="light" ${this.humanColor === "light" ? "selected" : ""}>${t.lightSide}</option>
                  <option value="dark" ${this.humanColor === "dark" ? "selected" : ""}>${t.darkSide}</option>
                </select>
              </label>
              <label class="field">
                <span>${t.aiLevel}</span>
                <select id="sel-level">
                  ${[1, 2, 3, 4, 5, 6, 7, 8]
                    .map((lvl) => `<option value="${lvl}" ${this.aiLevel === lvl ? "selected" : ""}>Level ${lvl}</option>`)
                    .join("")}
                </select>
              </label>
              `
                  : ""
              }
              <label class="field">
                <span>${t.timeControl}</span>
                <select id="sel-tc">
                  ${TIME_CONTROLS.map(
                    (tc) =>
                      `<option value="${tc.id}" ${this.timeControlId === tc.id ? "selected" : ""}>${
                        tc.id === "unlimited" ? t.unlimited : tc.label
                      }</option>`,
                  ).join("")}
                </select>
              </label>
            </div>
          </div>
          `
              : ""
          }

          <div class="card" id="analysis-card"></div>
          <div class="card" id="moves-card"></div>

          <div class="card">
            <div class="card-header">
              <h2 class="card-title">${t.fenPdn}</h2>
              <span class="toast" id="toast-msg">${this.toastMessage}</span>
            </div>
            <div class="io-box">
              <textarea id="io-text" placeholder="W:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32  or  PDN..."></textarea>
              <div class="io-actions">
                <button class="btn" id="btn-copy-fen">${t.copyFen}</button>
                <button class="btn" id="btn-load-fen">${t.loadFen}</button>
                <button class="btn" id="btn-copy-pdn">${t.exportPdn}</button>
                <button class="btn" id="btn-import-pdn">${t.importPdn}</button>
              </div>
            </div>
          </div>
        </aside>
      </main>
    `;

    this.boardMount = this.root.querySelector("#board-mount")!;
    this.evalFill = this.root.querySelector("#eval-fill")!;
    this.evalTopLabel = this.root.querySelector("#eval-top")!;
    this.evalBottomLabel = this.root.querySelector("#eval-bottom")!;
    this.topPlayerEl = this.root.querySelector("#top-player")!;
    this.bottomPlayerEl = this.root.querySelector("#bottom-player")!;
    this.statusEl = this.root.querySelector("#status-box")!;
    this.analysisPanelEl = this.root.querySelector("#analysis-card")!;
    this.movesPanelEl = this.root.querySelector("#moves-card")!;
    this.ioTextarea = this.root.querySelector("#io-text")!;
    this.toastEl = this.root.querySelector("#toast-msg")!;

    if (this.board) this.board.destroy();
    this.board = new MakhosBoard(
      this.boardMount,
      {
        pieces: this.currentSnapshot?.pieces ?? [],
        legalMoves: this.currentSnapshot?.legal_moves ?? [],
        hasCapture: this.currentSnapshot?.has_capture ?? false,
        orientation: this.orientation,
        showNumbers: this.showNumbers,
        showMainDiagonal: this.showMainDiagonal,
        interactive: this.canHumanMoveNow(),
        lastMove: this.timeline[this.cursor]?.lastMove ?? null,
        arrows: this.computeBoardArrows(),
      },
      (mv) => void this.handleUserMove(mv),
    );

    this.bindControls();
    this.refreshPanels();
  }

  private bindControls(): void {
    this.root.querySelectorAll<HTMLButtonElement>("[data-mode]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const nextMode = btn.dataset.mode as PlayMode;
        if (nextMode === this.mode) return;
        this.mode = nextMode;
        this.analysisEnabled = nextMode === "analysis";
        if (nextMode === "ai") {
          this.orientation = this.humanColor;
        }
        this.renderShell();
        void this.startNewGame();
      });
    });

    this.root.querySelector("#btn-locale")?.addEventListener("click", () => {
      this.locale = this.locale === "th" ? "en" : "th";
      this.renderShell();
    });

    this.root.querySelector("#btn-new")?.addEventListener("click", () => void this.startNewGame());
    this.root.querySelector("#btn-flip")?.addEventListener("click", () => {
      this.orientation = this.orientation === "light" ? "dark" : "light";
      this.syncBoard();
      this.renderPlayerBars();
      this.renderEvalBar();
    });
    this.root.querySelector("#btn-nums")?.addEventListener("click", () => {
      this.showNumbers = !this.showNumbers;
      this.root.querySelector("#btn-nums")?.classList.toggle("is-active", this.showNumbers);
      this.syncBoard();
    });
    this.root.querySelector("#btn-diag")?.addEventListener("click", () => {
      this.showMainDiagonal = !this.showMainDiagonal;
      this.root.querySelector("#btn-diag")?.classList.toggle("is-active", this.showMainDiagonal);
      this.syncBoard();
    });
    this.root.querySelector("#btn-undo")?.addEventListener("click", () => void this.handleUndo());
    this.root.querySelector("#btn-resign")?.addEventListener("click", () => this.handleResign());

    this.root.querySelector<HTMLSelectElement>("#sel-side")?.addEventListener("change", (ev) => {
      this.humanColor = (ev.target as HTMLSelectElement).value as Color;
      this.orientation = this.humanColor;
      void this.startNewGame();
    });
    this.root.querySelector<HTMLSelectElement>("#sel-level")?.addEventListener("change", (ev) => {
      this.aiLevel = Number((ev.target as HTMLSelectElement).value);
    });
    this.root.querySelector<HTMLSelectElement>("#sel-tc")?.addEventListener("change", (ev) => {
      this.timeControlId = (ev.target as HTMLSelectElement).value;
      void this.startNewGame();
    });

    this.root.querySelector("#btn-copy-fen")?.addEventListener("click", () => {
      const fen = this.currentSnapshot?.fen ?? "";
      this.ioTextarea.value = fen;
      void navigator.clipboard?.writeText(fen);
      this.showToast(this.t().copied);
    });

    this.root.querySelector("#btn-load-fen")?.addEventListener("click", () => {
      const fen = this.ioTextarea.value.trim();
      if (!fen) return;
      void this.loadCustomFen(fen);
    });

    this.root.querySelector("#btn-copy-pdn")?.addEventListener("click", async () => {
      const moves = this.timeline.slice(1).map((e) => e.notation!).filter(Boolean);
      const pdn = await this.engine.exportPdn({
        initialFen: this.timeline[0]?.fen,
        moves,
        event: "Limakhos Local Session",
        lightPlayer: this.mode === "ai" && this.humanColor === "dark" ? `AI L${this.aiLevel}` : "Light",
        darkPlayer: this.mode === "ai" && this.humanColor === "light" ? `AI L${this.aiLevel}` : "Dark",
      });
      this.ioTextarea.value = pdn;
      void navigator.clipboard?.writeText(pdn);
      this.showToast(this.t().copied);
    });

    this.root.querySelector("#btn-import-pdn")?.addEventListener("click", async () => {
      const pdn = this.ioTextarea.value.trim();
      if (!pdn) return;
      try {
        const res = await this.engine.importPdn(pdn);
        this.stopClock();
        this.forcedOutcome = null;
        this.timeline = res.fens.map((fen, idx) => ({
          fen,
          hash: res.hashes[idx] ?? "0",
          notation: idx === 0 ? null : res.notations[idx - 1],
          lastMove: null,
        }));
        this.cursor = this.timeline.length - 1;
        await this.inspectCursor();
        this.showToast("PDN loaded");
      } catch (err) {
        this.showToast(err instanceof Error ? err.message : String(err));
      }
    });
  }

  private async startNewGame(customFen?: string): Promise<void> {
    this.stopClock();
    this.forcedOutcome = null;
    this.aiThinking = false;
    this.lastAnalysis = null;
    await this.engine.clearCache();

    const snap = await this.engine.inspect(customFen);
    this.timeline = [{ fen: snap.fen, hash: snap.hash, notation: null, lastMove: null }];
    this.cursor = 0;
    this.currentSnapshot = snap;

    const tc = this.activeTimeControl();
    this.clocksMs = { light: tc.initialMs, dark: tc.initialMs };
    if (tc.initialMs > 0 && this.mode !== "analysis") {
      this.startClock();
    }

    this.syncBoard();
    this.refreshPanels();
    void this.triggerAnalysisIfEnabled();
    void this.maybeTriggerAiTurn();
  }

  private async loadCustomFen(fen: string): Promise<void> {
    try {
      await this.startNewGame(fen);
      this.showToast("FEN loaded");
    } catch (err) {
      this.showToast(err instanceof Error ? err.message : String(err));
    }
  }

  private activeTimeControl(): TimeControlOption {
    return TIME_CONTROLS.find((c) => c.id === this.timeControlId) ?? TIME_CONTROLS[0];
  }

  private effectiveOutcome(): GameOutcome {
    if (this.forcedOutcome) return this.forcedOutcome;
    return this.currentSnapshot?.outcome ?? { type: "ongoing" };
  }

  private isAtLatestPly(): boolean {
    return this.cursor === this.timeline.length - 1;
  }

  private canHumanMoveNow(): boolean {
    if (!this.currentSnapshot || this.aiThinking) return false;
    if (this.effectiveOutcome().type !== "ongoing") return false;
    if (this.mode === "analysis") return true;
    if (!this.isAtLatestPly()) return false;
    if (this.mode === "ai") {
      return this.currentSnapshot.side_to_move === this.humanColor;
    }
    return true;
  }

  private historyHashesUpTo(ply: number): string[] {
    return this.timeline.slice(0, ply + 1).map((e) => e.hash);
  }

  private async handleUserMove(move: MoveDto): Promise<void> {
    if (!this.currentSnapshot || !this.canHumanMoveNow()) return;
    await this.commitNotation(move.long_notation);
  }

  private async commitNotation(moveNotation: string): Promise<void> {
    if (!this.currentSnapshot) return;
    const mover = this.currentSnapshot.side_to_move;
    const hashes = this.historyHashesUpTo(this.cursor);
    const res = await this.engine.applyMove(this.currentSnapshot.fen, moveNotation, hashes);

    // Truncate future branch if stepping back in Analysis mode
    if (this.cursor < this.timeline.length - 1) {
      this.timeline = this.timeline.slice(0, this.cursor + 1);
    }

    this.timeline.push({
      fen: res.snapshot.fen,
      hash: res.snapshot.hash,
      notation: res.notation,
      lastMove: { from: res.applied.from, to: res.applied.to, path: res.applied.path },
    });
    this.cursor = this.timeline.length - 1;
    this.currentSnapshot = res.snapshot;
    this.lastAnalysis = null;

    // Add TCA increment to the player who just completed their move
    const tc = this.activeTimeControl();
    if (tc.initialMs > 0 && this.mode !== "analysis") {
      this.clocksMs[mover] += tc.incrementMs;
    }
    if (this.effectiveOutcome().type !== "ongoing") {
      this.stopClock();
    }

    this.syncBoard();
    this.refreshPanels();
    void this.triggerAnalysisIfEnabled();
    void this.maybeTriggerAiTurn();
  }

  private async maybeTriggerAiTurn(): Promise<void> {
    if (this.mode !== "ai" || !this.currentSnapshot) return;
    if (this.effectiveOutcome().type !== "ongoing") return;
    if (!this.isAtLatestPly()) return;
    if (this.currentSnapshot.side_to_move === this.humanColor) return;

    this.aiThinking = true;
    this.syncBoard();
    this.refreshPanels();

    const fen = this.currentSnapshot.fen;
    const hashes = this.historyHashesUpTo(this.cursor);
    try {
      const { chosen, analysis } = await this.engine.aiMove(fen, this.aiLevel, hashes);
      this.aiThinking = false;
      if (this.currentSnapshot?.fen !== fen || this.mode !== "ai") return;
      if (this.analysisEnabled) {
        this.lastAnalysis = analysis;
      }
      if (chosen) {
        await this.commitNotation(chosen.long_notation);
      }
    } catch {
      this.aiThinking = false;
      this.syncBoard();
      this.refreshPanels();
    }
  }

  private async triggerAnalysisIfEnabled(): Promise<void> {
    if (!this.analysisEnabled || !this.currentSnapshot) return;
    if (this.effectiveOutcome().type !== "ongoing") {
      this.lastAnalysis = null;
      this.syncBoard();
      this.renderAnalysisPanel();
      this.renderEvalBar();
      return;
    }

    const seq = ++this.analysisSeq;
    const fen = this.currentSnapshot.fen;
    const hashes = this.historyHashesUpTo(this.cursor);
    this.analysisBusy = true;
    this.renderAnalysisPanel();

    const t0 = performance.now();
    try {
      const res = await this.engine.analyze(fen, {
        maxDepth: this.analysisDepth,
        maxNodes: 350_000,
        multipv: this.analysisMultiPv,
        historyHashes: hashes,
      });
      if (seq !== this.analysisSeq) return;
      this.lastAnalysis = res;
      this.lastAnalysisMs = Math.max(1, Math.round(performance.now() - t0));
      this.analysisBusy = false;
      this.syncBoard();
      this.renderAnalysisPanel();
      this.renderEvalBar();
    } catch {
      if (seq !== this.analysisSeq) return;
      this.analysisBusy = false;
      this.renderAnalysisPanel();
    }
  }

  private async inspectCursor(): Promise<void> {
    const entry = this.timeline[this.cursor];
    if (!entry) return;
    const hashes = this.historyHashesUpTo(this.cursor);
    this.currentSnapshot = await this.engine.inspect(entry.fen, hashes);
    this.lastAnalysis = null;
    this.syncBoard();
    this.refreshPanels();
    void this.triggerAnalysisIfEnabled();
  }

  private async handleUndo(): Promise<void> {
    if (this.cursor === 0 || this.aiThinking) return;
    const steps = this.mode === "ai" && this.cursor >= 2 ? 2 : 1;
    this.timeline = this.timeline.slice(0, this.cursor - steps + 1);
    this.cursor = this.timeline.length - 1;
    this.forcedOutcome = null;
    await this.inspectCursor();
  }

  private handleResign(): void {
    if (!this.currentSnapshot || this.effectiveOutcome().type !== "ongoing") return;
    const loser = this.mode === "ai" ? this.humanColor : this.currentSnapshot.side_to_move;
    this.forcedOutcome = {
      type: "win",
      winner: loser === "light" ? "dark" : "light",
      reason: "resignation",
    };
    this.stopClock();
    this.syncBoard();
    this.refreshPanels();
  }

  private handleKeydown(ev: KeyboardEvent): void {
    if (ev.target instanceof HTMLTextAreaElement || ev.target instanceof HTMLInputElement) return;
    if (ev.key === "ArrowLeft" && this.cursor > 0) {
      this.cursor -= 1;
      void this.inspectCursor();
    } else if (ev.key === "ArrowRight" && this.cursor < this.timeline.length - 1) {
      this.cursor += 1;
      void this.inspectCursor();
    } else if (ev.key === "Home" && this.cursor !== 0) {
      this.cursor = 0;
      void this.inspectCursor();
    } else if (ev.key === "End" && this.cursor !== this.timeline.length - 1) {
      this.cursor = this.timeline.length - 1;
      void this.inspectCursor();
    } else if (ev.key.toLowerCase() === "f") {
      this.orientation = this.orientation === "light" ? "dark" : "light";
      this.syncBoard();
      this.renderPlayerBars();
      this.renderEvalBar();
    }
  }

  private startClock(): void {
    this.stopClock();
    this.lastTick = performance.now();
    this.clockTimer = window.setInterval(() => {
      if (!this.currentSnapshot || this.effectiveOutcome().type !== "ongoing") {
        this.stopClock();
        return;
      }
      const now = performance.now();
      const dt = now - this.lastTick;
      this.lastTick = now;
      const side = this.currentSnapshot.side_to_move;
      this.clocksMs[side] = Math.max(0, this.clocksMs[side] - dt);
      if (this.clocksMs[side] <= 0) {
        this.forcedOutcome = {
          type: "win",
          winner: side === "light" ? "dark" : "light",
          reason: "timeout",
        };
        this.stopClock();
        this.syncBoard();
        this.refreshPanels();
        return;
      }
      this.renderPlayerBars();
    }, 100);
  }

  private stopClock(): void {
    if (this.clockTimer !== null) {
      clearInterval(this.clockTimer);
      this.clockTimer = null;
    }
  }

  private computeBoardArrows(): BoardArrow[] {
    if (!this.analysisEnabled || !this.lastAnalysis || this.lastAnalysis.lines.length === 0) {
      return [];
    }
    const palette = ["#22c55e", "#38bdf8", "#f59e0b"];
    return this.lastAnalysis.lines.slice(0, 3).map((line, i) => ({
      from: line.best_move.from,
      to: line.best_move.to,
      path: line.best_move.path,
      color: palette[i] ?? "#38bdf8",
      opacity: i === 0 ? 0.84 : 0.48,
      width: i === 0 ? 15 : 10,
    }));
  }

  private syncBoard(): void {
    this.board.setState({
      pieces: this.currentSnapshot?.pieces ?? [],
      legalMoves: this.currentSnapshot?.legal_moves ?? [],
      hasCapture: this.currentSnapshot?.has_capture ?? false,
      orientation: this.orientation,
      showNumbers: this.showNumbers,
      showMainDiagonal: this.showMainDiagonal,
      interactive: this.canHumanMoveNow(),
      lastMove: this.timeline[this.cursor]?.lastMove ?? null,
      arrows: this.computeBoardArrows(),
    });
  }

  private refreshPanels(): void {
    this.renderPlayerBars();
    this.renderStatusBanner();
    this.renderEvalBar();
    this.renderAnalysisPanel();
    this.renderMovesPanel();
  }

  private renderPlayerBars(): void {
    const t = this.t();
    const topColor: Color = this.orientation === "light" ? "dark" : "light";
    const bottomColor: Color = this.orientation;
    const tc = this.activeTimeControl();
    const activeTurn =
      this.effectiveOutcome().type === "ongoing" ? this.currentSnapshot?.side_to_move : null;

    const labelFor = (c: Color) => {
      if (this.mode === "ai" && c !== this.humanColor) {
        return `${c === "light" ? t.lightSide : t.darkSide} — AI L${this.aiLevel}`;
      }
      return c === "light" ? t.lightSide : t.darkSide;
    };

    const renderBar = (el: HTMLElement, c: Color) => {
      const running = tc.initialMs > 0 && this.mode !== "analysis" && activeTurn === c;
      const low = running && this.clocksMs[c] < 15_000;
      const clockText = tc.initialMs > 0 && this.mode !== "analysis" ? formatClockMs(this.clocksMs[c]) : "∞";
      el.innerHTML = `
        <div class="player-name">
          <span class="color-dot is-${c}"></span>
          <span>${labelFor(c)}</span>
        </div>
        <div class="clock ${running ? "is-running" : ""} ${low ? "is-low" : ""}">${clockText}</div>
      `;
    };

    renderBar(this.topPlayerEl, topColor);
    renderBar(this.bottomPlayerEl, bottomColor);
  }

  private renderStatusBanner(): void {
    const t = this.t();
    const snap = this.currentSnapshot;
    const outcome = this.effectiveOutcome();

    let text = "";
    if (outcome.type === "win") {
      const who = outcome.winner === "light" ? t.winnerLight : t.winnerDark;
      const reasonMap = {
        no_pieces: t.outcomeWinNoPieces,
        no_legal_moves: t.outcomeWinNoMoves,
        resignation: t.outcomeWinResign,
        timeout: t.outcomeWinTimeout,
      };
      text = `${who} (${reasonMap[outcome.reason]})`;
    } else if (outcome.type === "draw") {
      text =
        outcome.reason === "threefold_repetition"
          ? t.outcomeDrawRepetition
          : t.outcomeDraw16Moves;
    } else if (snap) {
      text = snap.side_to_move === "light" ? t.turnLight : t.turnDark;
      if (this.aiThinking) text += " — AI...";
    }

    const kingFullMoves = snap ? Math.floor(snap.king_only_plies / 2) : 0;
    const capturePill =
      snap && snap.has_capture && outcome.type === "ongoing"
        ? `<span class="pill is-warn">${t.mandatoryCapture}</span>`
        : "";
    const drawPill =
      kingFullMoves > 0
        ? `<span class="pill ${kingFullMoves >= 12 ? "is-warn" : ""}">${t.sixteenMoveCounter}: ${kingFullMoves}/16</span>`
        : "";

    this.statusEl.innerHTML = `
      <div class="status-banner ${outcome.type !== "ongoing" ? "is-over" : ""}">
        <span>${text}</span>
        <div style="display:flex;gap:6px;flex-wrap:wrap;">${capturePill}${drawPill}</div>
      </div>
    `;
  }

  private renderEvalBar(): void {
    const topLine = this.lastAnalysis?.lines[0];
    const snap = this.currentSnapshot;
    if (!this.analysisEnabled || !topLine || !snap) {
      this.evalFill.style.height = "50%";
      this.evalTopLabel.textContent = "0.0";
      this.evalBottomLabel.textContent = "";
      return;
    }

    const lightPct = evalToLightPercent(topLine.score_light_cp);
    // If Light is on bottom, Light fills from bottom; if Dark is on bottom, flip fill
    const fillPct = this.orientation === "light" ? lightPct : 100 - lightPct;
    this.evalFill.style.height = `${fillPct}%`;

    const formatted = formatEvalLight(topLine.score_light_cp, topLine.mate_in, snap.side_to_move);
    if (topLine.score_light_cp >= 0) {
      this.evalBottomLabel.textContent = this.orientation === "light" ? formatted : "";
      this.evalTopLabel.textContent = this.orientation === "light" ? "" : formatted;
    } else {
      this.evalTopLabel.textContent = this.orientation === "light" ? formatted : "";
      this.evalBottomLabel.textContent = this.orientation === "light" ? "" : formatted;
    }
  }

  private renderAnalysisPanel(): void {
    const t = this.t();
    const snap = this.currentSnapshot;
    const res = this.lastAnalysis;
    const nps = res && this.lastAnalysisMs > 0 ? Math.round((res.nodes * 1000) / this.lastAnalysisMs) : 0;

    const linesHtml =
      this.analysisEnabled && res && snap && res.lines.length > 0
        ? res.lines
            .map((line) => {
              const badge = formatEvalLight(line.score_light_cp, line.mate_in, snap.side_to_move);
              const scoreCls = line.score_light_cp >= 0 ? "is-light" : "is-dark";
              const bestToken = line.pv_notations[0] ?? "";
              return `
                <button class="pv-row" data-pv-move="${bestToken}">
                  <span class="pv-score ${scoreCls}">${badge}</span>
                  <span class="pv-line">${line.pv_notations.join("  ")}</span>
                </button>
              `;
            })
            .join("")
        : `<div style="color:var(--muted);font-size:0.84rem;">${
            this.analysisEnabled ? (this.analysisBusy ? t.engineThinking : "—") : t.engineOff
          }</div>`;

    this.analysisPanelEl.innerHTML = `
      <div class="card-header">
        <h2 class="card-title">${t.engineAnalysis}</h2>
        <div style="display:flex;gap:6px;align-items:center;">
          <select id="sel-depth" class="btn" style="padding:3px 6px;">
            ${[5, 6, 8, 10, 12]
              .map((d) => `<option value="${d}" ${this.analysisDepth === d ? "selected" : ""}>D${d}</option>`)
              .join("")}
          </select>
          <button class="btn ${this.analysisEnabled ? "is-active" : ""}" id="btn-toggle-engine">
            ${this.analysisEnabled ? t.engineOn : t.engineOff}
          </button>
        </div>
      </div>
      ${
        this.analysisEnabled && res
          ? `<div class="analysis-meta">
               <span>${t.depth}: ${res.depth}</span>
               <span>${t.nodes}: ${res.nodes.toLocaleString()}</span>
               <span>${t.nps}: ${nps.toLocaleString()}</span>
             </div>`
          : ""
      }
      <div class="pv-list">${linesHtml}</div>
    `;

    this.analysisPanelEl.querySelector("#btn-toggle-engine")?.addEventListener("click", () => {
      this.analysisEnabled = !this.analysisEnabled;
      this.syncBoard();
      this.renderEvalBar();
      this.renderAnalysisPanel();
      if (this.analysisEnabled) void this.triggerAnalysisIfEnabled();
    });

    this.analysisPanelEl.querySelector<HTMLSelectElement>("#sel-depth")?.addEventListener("change", (ev) => {
      this.analysisDepth = Number((ev.target as HTMLSelectElement).value);
      void this.triggerAnalysisIfEnabled();
    });

    this.analysisPanelEl.querySelectorAll<HTMLButtonElement>("[data-pv-move]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const tok = btn.dataset.pvMove;
        if (tok && this.canHumanMoveNow()) {
          void this.commitNotation(tok);
        }
      });
    });
  }

  private renderMovesPanel(): void {
    const t = this.t();
    const moves = this.timeline.slice(1);
    let rowsHtml = "";
    if (moves.length === 0) {
      rowsHtml = `<div style="grid-column:1/-1;color:var(--muted);padding:4px 0;">${t.noMovesYet}</div>`;
    } else {
      for (let i = 0; i < moves.length; i += 2) {
        const moveNum = Math.floor(i / 2) + 1;
        const lightPly = i + 1;
        const darkPly = i + 2;
        const lightTok = moves[i]?.notation ?? "";
        const darkTok = moves[i + 1]?.notation ?? "";
        rowsHtml += `
          <span class="move-num">${moveNum}.</span>
          <button class="move-cell ${this.cursor === lightPly ? "is-current" : ""}" data-ply="${lightPly}">${lightTok}</button>
          ${
            darkTok
              ? `<button class="move-cell ${this.cursor === darkPly ? "is-current" : ""}" data-ply="${darkPly}">${darkTok}</button>`
              : `<span></span>`
          }
        `;
      }
    }

    this.movesPanelEl.innerHTML = `
      <div class="card-header">
        <h2 class="card-title">${t.movesHistory}</h2>
        <span style="font-size:0.78rem;color:var(--muted);">${this.cursor}/${this.timeline.length - 1}</span>
      </div>
      <div class="moves-list">${rowsHtml}</div>
      <div class="nav-row">
        <button class="btn" id="nav-start" ${this.cursor === 0 ? "disabled" : ""}>|&lt;</button>
        <button class="btn" id="nav-prev" ${this.cursor === 0 ? "disabled" : ""}>&lt;</button>
        <button class="btn" id="nav-next" ${this.cursor >= this.timeline.length - 1 ? "disabled" : ""}>&gt;</button>
        <button class="btn" id="nav-end" ${this.cursor >= this.timeline.length - 1 ? "disabled" : ""}>&gt;|</button>
      </div>
    `;

    this.movesPanelEl.querySelectorAll<HTMLButtonElement>("[data-ply]").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.cursor = Number(btn.dataset.ply);
        void this.inspectCursor();
      });
    });

    this.movesPanelEl.querySelector("#nav-start")?.addEventListener("click", () => {
      this.cursor = 0;
      void this.inspectCursor();
    });
    this.movesPanelEl.querySelector("#nav-prev")?.addEventListener("click", () => {
      if (this.cursor > 0) {
        this.cursor -= 1;
        void this.inspectCursor();
      }
    });
    this.movesPanelEl.querySelector("#nav-next")?.addEventListener("click", () => {
      if (this.cursor < this.timeline.length - 1) {
        this.cursor += 1;
        void this.inspectCursor();
      }
    });
    this.movesPanelEl.querySelector("#nav-end")?.addEventListener("click", () => {
      this.cursor = this.timeline.length - 1;
      void this.inspectCursor();
    });
  }

  private showToast(msg: string): void {
    this.toastMessage = msg;
    if (this.toastEl) this.toastEl.textContent = msg;
    window.setTimeout(() => {
      if (this.toastMessage === msg) {
        this.toastMessage = "";
        if (this.toastEl) this.toastEl.textContent = "";
      }
    }, 2500);
  }
}

const mount = document.querySelector<HTMLDivElement>("#app");
if (mount) {
  new LimakhosApp(mount);
}
