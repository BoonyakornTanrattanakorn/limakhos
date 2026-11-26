import "./styles/app.css";
import { MakhosBoard, type BoardArrow } from "./board/MakhosBoard";
import { MakhosEngineClient } from "./engine/client";
import type { AnalysisResult, Color, GameOutcome, MoveDto, PositionSnapshot } from "./engine/types";
import { MESSAGES, type Locale } from "./i18n/messages";
import {
  OnlineClient,
  type GameSummary,
  type ParticipantRole,
  type RoomConnection,
  type RoomStateSnapshot,
  type RoomSummary,
  type UserRecord,
} from "./online/client";
import { FORMATIONS, OPENING_BOOK, matchOpeningHistory } from "./study/openings";
import { evalToLightPercent, formatClockMs, formatEvalLight } from "./ui/eval";

type PlayMode = "analysis" | "ai" | "local" | "online";

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

const LS_TOKEN_KEY = "limakhos_auth_token";
const LS_GUEST_KEY = "limakhos_guest_name";

interface HistoryEntry {
  fen: string;
  hash: string;
  notation: string | null;
  lastMove: { from: number; to: number; path?: number[] } | null;
}

function escapeHtml(raw: string): string {
  return raw
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

class LimakhosApp {
  private engine = new MakhosEngineClient();
  private online = new OnlineClient();
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

  // Online multiplayer state
  private authToken: string | null = localStorage.getItem(LS_TOKEN_KEY);
  private authUser: UserRecord | null = null;
  private guestName: string = localStorage.getItem(LS_GUEST_KEY) || "Guest";
  private onlineTimeControl = "5+3";
  private onlineError: string | null = null;
  private openRooms: RoomSummary[] = [];
  private leaderboardPlayers: UserRecord[] = [];
  private recentGames: GameSummary[] = [];
  private onlineRoomId: string | null = null;
  private onlineRole: ParticipantRole | null = null;
  private onlineState: RoomStateSnapshot | null = null;
  private onlineConn: RoomConnection | null = null;
  private onlineClockAnchorMs: Record<Color, number> = { light: 0, dark: 0 };
  private onlineClockAnchorPerf = 0;
  private onlineSyncSeq = 0;

  // DOM refs
  private root: HTMLElement;
  private boardMount!: HTMLElement;
  private evalFill!: HTMLElement;
  private evalTopLabel!: HTMLElement;
  private evalBottomLabel!: HTMLElement;
  private topPlayerEl!: HTMLElement;
  private bottomPlayerEl!: HTMLElement;
  private statusEl!: HTMLElement;
  private onlinePanelEl!: HTMLElement | null;
  private analysisPanelEl!: HTMLElement;
  private openingPanelEl!: HTMLElement | null;
  private movesPanelEl!: HTMLElement;
  private ioTextarea!: HTMLTextAreaElement;
  private toastEl!: HTMLElement;

  constructor(root: HTMLElement) {
    this.root = root;
    const urlRoom = new URLSearchParams(window.location.search).get("room")?.trim();
    if (urlRoom) {
      this.mode = "online";
      this.analysisEnabled = false;
    }

    this.renderShell();
    window.addEventListener("keydown", (ev) => this.handleKeydown(ev));

    if (urlRoom) {
      void this.initOnlineFromUrl(urlRoom);
    } else {
      void this.startNewGame();
    }
  }

  private t() {
    return MESSAGES[this.locale];
  }

  private async initOnlineFromUrl(roomId: string): Promise<void> {
    await this.startNewGame();
    await this.refreshOnlineLobby();
    this.joinOnlineRoom(roomId, "player");
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
            <button class="tab-btn ${this.mode === "online" ? "is-active" : ""}" data-mode="online">${t.modeOnline}</button>
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
              ${
                this.mode === "online" && this.onlineRoomId
                  ? `<button class="btn is-danger" id="btn-leave-room">${t.leaveRoom}</button>`
                  : `<button class="btn is-primary" id="btn-new">${t.newGame}</button>`
              }
              <button class="btn" id="btn-flip">${t.flipBoard}</button>
              <button class="btn ${this.showNumbers ? "is-active" : ""}" id="btn-nums">${t.numbersToggle}</button>
              <button class="btn ${this.showMainDiagonal ? "is-active" : ""}" id="btn-diag">${t.mainDiagToggle}</button>
            </div>
            <div class="toolbar-group">
              <button class="btn" id="btn-undo" ${this.mode === "online" ? "disabled" : ""}>${t.undo}</button>
              <button class="btn is-danger" id="btn-resign">${t.resign}</button>
            </div>
          </div>
        </section>

        <aside class="side-column">
          <div id="status-box"></div>
          ${this.mode === "online" ? `<div id="online-panel" class="online-stack"></div>` : ""}

          ${
            this.mode === "ai" || this.mode === "local"
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
          ${this.mode === "analysis" ? `<div class="card" id="opening-card"></div>` : ""}
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
                <button class="btn" id="btn-load-fen" ${this.mode === "online" ? "disabled" : ""}>${t.loadFen}</button>
                <button class="btn" id="btn-copy-pdn">${t.exportPdn}</button>
                <button class="btn" id="btn-import-pdn" ${this.mode === "online" ? "disabled" : ""}>${t.importPdn}</button>
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
    this.onlinePanelEl = this.root.querySelector("#online-panel");
    this.analysisPanelEl = this.root.querySelector("#analysis-card")!;
    this.openingPanelEl = this.root.querySelector("#opening-card");
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
        this.leaveOnlineRoom(false);
        this.mode = nextMode;
        this.analysisEnabled = nextMode === "analysis";
        if (nextMode === "ai") {
          this.orientation = this.humanColor;
        }
        this.renderShell();
        if (nextMode === "online") {
          void this.startNewGame();
          void this.refreshOnlineLobby();
        } else {
          void this.startNewGame();
        }
      });
    });

    this.root.querySelector("#btn-locale")?.addEventListener("click", () => {
      this.locale = this.locale === "th" ? "en" : "th";
      this.renderShell();
    });

    this.root.querySelector("#btn-new")?.addEventListener("click", () => void this.startNewGame());
    this.root.querySelector("#btn-leave-room")?.addEventListener("click", () => {
      this.leaveOnlineRoom(true);
    });
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
      if (!fen || this.mode === "online") return;
      void this.loadCustomFen(fen);
    });

    this.root.querySelector("#btn-copy-pdn")?.addEventListener("click", async () => {
      if (this.mode === "online" && this.onlineState) {
        this.ioTextarea.value = this.onlineState.pdn;
        void navigator.clipboard?.writeText(this.onlineState.pdn);
        this.showToast(this.t().copied);
        return;
      }
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
      if (!pdn || this.mode === "online") return;
      await this.importPdnText(pdn);
    });
  }

  private async importPdnText(pdn: string, switchToAnalysis = false): Promise<void> {
    try {
      const res = await this.engine.importPdn(pdn);
      if (switchToAnalysis) {
        this.leaveOnlineRoom(false);
        this.mode = "analysis";
        this.analysisEnabled = true;
        this.renderShell();
      }
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
    if (tc.initialMs > 0 && (this.mode === "ai" || this.mode === "local")) {
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
    if (this.mode === "online" && this.onlineState) {
      return this.onlineState.outcome;
    }
    if (this.forcedOutcome) return this.forcedOutcome;
    return this.currentSnapshot?.outcome ?? { type: "ongoing" };
  }

  private isAtLatestPly(): boolean {
    return this.cursor === this.timeline.length - 1;
  }

  private canHumanMoveNow(): boolean {
    if (!this.currentSnapshot || this.aiThinking) return false;
    if (this.effectiveOutcome().type !== "ongoing") return false;
    if (this.mode === "online") {
      if (!this.onlineState || this.onlineState.status !== "active") return false;
      if (!this.isAtLatestPly()) return false;
      return this.onlineRole === this.onlineState.side_to_move;
    }
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
    if (this.mode === "online") {
      this.onlineConn?.sendMove(move.long_notation);
      return;
    }
    await this.commitNotation(move.long_notation);
  }

  private async commitNotation(moveNotation: string): Promise<void> {
    if (!this.currentSnapshot) return;
    const mover = this.currentSnapshot.side_to_move;
    const hashes = this.historyHashesUpTo(this.cursor);
    const res = await this.engine.applyMove(this.currentSnapshot.fen, moveNotation, hashes);

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

    const tc = this.activeTimeControl();
    if (tc.initialMs > 0 && (this.mode === "ai" || this.mode === "local")) {
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
    if (this.mode === "online" && this.onlineState?.status === "active") {
      this.lastAnalysis = null;
      this.renderAnalysisPanel();
      this.renderEvalBar();
      return;
    }
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
    if (this.mode === "online" && this.onlineState && this.isAtLatestPly()) {
      this.currentSnapshot = this.snapshotFromOnline(this.onlineState);
      this.syncBoard();
      this.refreshPanels();
      return;
    }
    const hashes = this.historyHashesUpTo(this.cursor);
    this.currentSnapshot = await this.engine.inspect(entry.fen, hashes);
    this.lastAnalysis = null;
    this.syncBoard();
    this.refreshPanels();
    void this.triggerAnalysisIfEnabled();
  }

  private async handleUndo(): Promise<void> {
    if (this.mode === "online" || this.cursor === 0 || this.aiThinking) return;
    const steps = this.mode === "ai" && this.cursor >= 2 ? 2 : 1;
    this.timeline = this.timeline.slice(0, this.cursor - steps + 1);
    this.cursor = this.timeline.length - 1;
    this.forcedOutcome = null;
    await this.inspectCursor();
  }

  private handleResign(): void {
    if (this.mode === "online") {
      if (this.onlineRoomId && this.onlineRole !== "spectator") {
        this.onlineConn?.resign();
      }
      return;
    }
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
      if (this.mode === "online") {
        if (!this.onlineState || this.onlineState.status !== "active" || this.onlineState.initial_ms === 0) {
          this.stopClock();
          return;
        }
        const elapsed = performance.now() - this.onlineClockAnchorPerf;
        const turn = this.onlineState.side_to_move;
        this.clocksMs = {
          light:
            turn === "light"
              ? Math.max(0, this.onlineClockAnchorMs.light - elapsed)
              : this.onlineClockAnchorMs.light,
          dark:
            turn === "dark"
              ? Math.max(0, this.onlineClockAnchorMs.dark - elapsed)
              : this.onlineClockAnchorMs.dark,
        };
        this.renderPlayerBars();
        return;
      }

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

  // --- Online Multiplayer Methods ---

  private async refreshOnlineLobby(): Promise<void> {
    this.onlineError = null;
    try {
      await this.online.health();
    } catch {
      this.onlineError = this.t().onlineOfflineError;
      this.renderOnlinePanel();
      return;
    }

    if (this.authToken) {
      try {
        const me = await this.online.me(this.authToken);
        this.authUser = me.user;
      } catch {
        this.authToken = null;
        this.authUser = null;
        localStorage.removeItem(LS_TOKEN_KEY);
      }
    } else {
      this.authUser = null;
    }

    const [roomsRes, lbRes, gamesRes] = await Promise.allSettled([
      this.online.listRooms(),
      this.online.leaderboard(20),
      this.online.recentGames(15),
    ]);
    if (roomsRes.status === "fulfilled") this.openRooms = roomsRes.value;
    if (lbRes.status === "fulfilled") this.leaderboardPlayers = lbRes.value;
    if (gamesRes.status === "fulfilled") this.recentGames = gamesRes.value;
    this.renderOnlinePanel();
  }

  private async handleAuthAction(kind: "login" | "register", username: string, password: string): Promise<void> {
    this.onlineError = null;
    try {
      const res =
        kind === "register"
          ? await this.online.register(username, password)
          : await this.online.login(username, password);
      this.authToken = res.token;
      this.authUser = res.user;
      localStorage.setItem(LS_TOKEN_KEY, res.token);
      this.showToast(`${res.user.username}`);
      await this.refreshOnlineLobby();
    } catch (err) {
      this.onlineError = err instanceof Error ? err.message : String(err);
      this.renderOnlinePanel();
    }
  }

  private handleLogout(): void {
    this.authToken = null;
    this.authUser = null;
    localStorage.removeItem(LS_TOKEN_KEY);
    this.renderOnlinePanel();
  }

  private async startOnlineRoom(mode: "private" | "matchmake", timeControl: string): Promise<void> {
    this.onlineError = null;
    try {
      const resp = await this.online.createRoom(
        {
          mode,
          time_control: timeControl,
          rated: Boolean(this.authToken),
          guest_name: this.authToken ? undefined : this.guestName,
        },
        this.authToken,
      );
      this.joinOnlineRoom(resp.room.id, "player");
    } catch (err) {
      this.onlineError =
        err instanceof TypeError
          ? this.t().onlineOfflineError
          : err instanceof Error
            ? err.message
            : String(err);
      this.renderOnlinePanel();
    }
  }

  private joinOnlineRoom(roomId: string, role: "player" | "spectator"): void {
    const cleanId = roomId.trim();
    if (!cleanId) return;
    this.leaveOnlineRoom(false);
    this.onlineRoomId = cleanId;
    this.onlineError = null;
    this.updateUrlRoomParam(cleanId);

    this.onlineConn = this.online.connectRoom(
      cleanId,
      {
        token: this.authToken,
        guestName: this.authToken ? undefined : this.guestName,
        role,
      },
      {
        onWelcome: (msg) => {
          this.onlineRole = msg.your_role;
          this.orientation = msg.your_role === "dark" ? "dark" : "light";
          this.renderShell();
          this.applyOnlineSnapshot(msg.state);
        },
        onState: (state) => {
          this.applyOnlineSnapshot(state);
        },
        onError: (message) => {
          this.onlineError = message;
          this.showToast(message);
          this.renderOnlinePanel();
        },
        onClose: () => {
          this.stopClock();
        },
      },
    );
    this.renderShell();
  }

  private leaveOnlineRoom(resetBoard: boolean): void {
    if (this.onlineConn) {
      this.onlineConn.close();
      this.onlineConn = null;
    }
    this.stopClock();
    this.onlineRoomId = null;
    this.onlineRole = null;
    this.onlineState = null;
    this.updateUrlRoomParam(null);
    if (resetBoard) {
      this.renderShell();
      void this.startNewGame();
      void this.refreshOnlineLobby();
    }
  }

  private updateUrlRoomParam(roomId: string | null): void {
    const url = new URL(window.location.href);
    if (roomId) {
      url.searchParams.set("room", roomId);
    } else {
      url.searchParams.delete("room");
    }
    window.history.replaceState({}, "", url.toString());
  }

  private snapshotFromOnline(state: RoomStateSnapshot): PositionSnapshot {
    return {
      fen: state.fen,
      side_to_move: state.side_to_move,
      king_only_plies: state.king_only_plies,
      fullmove_number: state.fullmove_number,
      hash: state.hash,
      pieces: state.pieces,
      has_capture: state.has_capture,
      legal_moves: state.legal_moves,
      outcome: state.outcome,
    };
  }

  private applyOnlineSnapshot(state: RoomStateSnapshot): void {
    const prevStatus = this.onlineState?.status;
    this.onlineState = state;
    this.onlineClockAnchorMs = {
      light: state.clocks_ms.light,
      dark: state.clocks_ms.dark,
    };
    this.onlineClockAnchorPerf = performance.now();
    this.clocksMs = { ...this.onlineClockAnchorMs };

    const wasAtLatest = this.isAtLatestPly();
    const expectedLen = state.notations.length + 1;
    if (state.notations.length === 0) {
      this.timeline = [{ fen: state.fen, hash: state.hash, notation: null, lastMove: null }];
      this.cursor = 0;
    } else if (this.timeline.length === state.notations.length) {
      this.timeline.push({
        fen: state.fen,
        hash: state.hash,
        notation: state.notations[state.notations.length - 1],
        lastMove: state.last_move,
      });
      if (wasAtLatest) {
        this.cursor = this.timeline.length - 1;
      }
    } else if (this.timeline.length !== expectedLen) {
      // Reconnected or joined mid-game: hydrate full history via local WASM PDN importer
      const seq = ++this.onlineSyncSeq;
      this.timeline = state.notations.map((tok, i) => ({
        fen: state.fen,
        hash: state.hash,
        notation: tok,
        lastMove: i === state.notations.length - 1 ? state.last_move : null,
      }));
      this.timeline.unshift({ fen: "", hash: "", notation: null, lastMove: null });
      this.cursor = this.timeline.length - 1;
      void this.engine.importPdn(state.pdn).then((res) => {
        if (seq !== this.onlineSyncSeq || this.onlineRoomId !== state.room_id) return;
        this.timeline = res.fens.map((fen, idx) => ({
          fen,
          hash: res.hashes[idx] ?? "0",
          notation: idx === 0 ? null : res.notations[idx - 1],
          lastMove: idx === res.fens.length - 1 ? state.last_move : null,
        }));
        if (this.isAtLatestPly()) {
          this.cursor = this.timeline.length - 1;
        }
        this.renderMovesPanel();
      });
    }

    if (this.isAtLatestPly()) {
      this.currentSnapshot = this.snapshotFromOnline(state);
    }

    if (state.status === "active" && state.initial_ms > 0) {
      this.startClock();
    } else {
      this.stopClock();
    }

    if (prevStatus !== "finished" && state.status === "finished") {
      void this.refreshOnlineLobby();
    }

    this.syncBoard();
    this.refreshPanels();
  }

  // --- Rendering ---

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
    if (this.mode === "online") {
      this.renderOnlinePanel();
    }
    this.renderEvalBar();
    this.renderAnalysisPanel();
    this.renderOpeningPanel();
    this.renderMovesPanel();
  }

  private renderPlayerBars(): void {
    const t = this.t();
    const topColor: Color = this.orientation === "light" ? "dark" : "light";
    const bottomColor: Color = this.orientation;
    const tc = this.activeTimeControl();

    if (this.mode === "online" && this.onlineState) {
      const s = this.onlineState;
      const activeTurn = s.status === "active" && s.outcome.type === "ongoing" ? s.side_to_move : null;
      const renderOnlineBar = (el: HTMLElement, c: Color) => {
        const seat = c === "light" ? s.light : s.dark;
        const name = seat ? escapeHtml(seat.name) : `${c === "light" ? t.lightSide : t.darkSide} (${t.waitingOpponent})`;
        const ratingBadge =
          seat && seat.rating !== null ? `<span class="badge-muted">(${Math.round(seat.rating)})</span>` : "";
        const offlineBadge =
          seat && !seat.connected ? `<span class="pill is-warn">${t.disconnected}</span>` : "";
        const running = s.initial_ms > 0 && activeTurn === c;
        const low = running && this.clocksMs[c] < 15_000;
        const clockText = s.initial_ms > 0 ? formatClockMs(this.clocksMs[c]) : "∞";
        el.innerHTML = `
          <div class="player-name">
            <span class="color-dot is-${c}"></span>
            <span>${name}</span>
            ${ratingBadge}
            ${offlineBadge}
          </div>
          <div class="clock ${running ? "is-running" : ""} ${low ? "is-low" : ""}">${clockText}</div>
        `;
      };
      renderOnlineBar(this.topPlayerEl, topColor);
      renderOnlineBar(this.bottomPlayerEl, bottomColor);
      return;
    }

    const activeTurn =
      this.effectiveOutcome().type === "ongoing" ? this.currentSnapshot?.side_to_move : null;

    const labelFor = (c: Color) => {
      if (this.mode === "ai" && c !== this.humanColor) {
        return `${c === "light" ? t.lightSide : t.darkSide} — AI L${this.aiLevel}`;
      }
      return c === "light" ? t.lightSide : t.darkSide;
    };

    const renderBar = (el: HTMLElement, c: Color) => {
      const timed = tc.initialMs > 0 && (this.mode === "ai" || this.mode === "local");
      const running = timed && activeTurn === c;
      const low = running && this.clocksMs[c] < 15_000;
      const clockText = timed ? formatClockMs(this.clocksMs[c]) : "∞";
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
    } else if (this.mode === "online" && this.onlineState && this.onlineState.status === "waiting") {
      text = t.waitingOpponent;
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
    const specPill =
      this.mode === "online" && this.onlineState
        ? `<span class="pill">${
            this.onlineRole === "spectator" ? t.spectating : `${t.spectators}: ${this.onlineState.spectators}`
          }</span>`
        : "";

    let ratingPill = "";
    if (this.mode === "online" && this.onlineState?.rating_changes) {
      const rc = this.onlineState.rating_changes;
      const dl = Math.round(rc.light_after - rc.light_before);
      const dd = Math.round(rc.dark_after - rc.dark_before);
      ratingPill = `<span class="pill">L ${dl >= 0 ? "+" : ""}${dl} / D ${dd >= 0 ? "+" : ""}${dd}</span>`;
    }

    this.statusEl.innerHTML = `
      <div class="status-banner ${outcome.type !== "ongoing" ? "is-over" : ""}">
        <span>${text}</span>
        <div style="display:flex;gap:6px;flex-wrap:wrap;">${capturePill}${drawPill}${specPill}${ratingPill}</div>
      </div>
    `;
  }

  private renderOnlinePanel(): void {
    if (!this.onlinePanelEl) return;
    const t = this.t();

    const alertHtml = this.onlineError
      ? `<div class="online-alert">${escapeHtml(this.onlineError)}</div>`
      : "";

    const inviteHtml = this.onlineRoomId
      ? (() => {
          const url = `${window.location.origin}${window.location.pathname}?room=${encodeURIComponent(this.onlineRoomId)}`;
          return `
            <div class="card">
              <div class="card-header">
                <h2 class="card-title">Room ${escapeHtml(this.onlineRoomId)}</h2>
                <button class="btn is-danger" id="btn-inline-leave">${t.leaveRoom}</button>
              </div>
              <div class="invite-bar">
                <code>${escapeHtml(url)}</code>
                <button class="btn is-primary" id="btn-copy-invite">${t.copyInvite}</button>
              </div>
              ${
                this.onlineState?.status === "finished"
                  ? `<div style="margin-top:8px;"><button class="btn is-primary" id="btn-analyze-finished">${t.analyzeGame}</button></div>`
                  : ""
              }
            </div>
          `;
        })()
      : "";

    const accountHtml = this.authUser
      ? `
        <div class="card">
          <div class="card-header">
            <h2 class="card-title">${t.onlineAccount}</h2>
            <button class="btn" id="btn-logout">${t.logout}</button>
          </div>
          <div class="inline-row" style="justify-content:space-between;">
            <strong>${escapeHtml(this.authUser.username)}</strong>
            <span class="pill">${t.ratingLabel}: ${Math.round(this.authUser.rating)} (${this.authUser.wins}W-${this.authUser.losses}L-${this.authUser.draws}D)</span>
          </div>
        </div>
      `
      : `
        <div class="card">
          <div class="card-header">
            <h2 class="card-title">${t.onlineAccount}</h2>
          </div>
          <div class="online-stack">
            <label class="field">
              <span>${t.guestName}</span>
              <input id="inp-guest-name" value="${escapeHtml(this.guestName)}" maxlength="24" />
            </label>
            <div class="inline-row">
              <input id="inp-auth-user" placeholder="${t.username}" maxlength="24" />
              <input id="inp-auth-pass" type="password" placeholder="${t.password}" />
            </div>
            <div class="inline-row">
              <button class="btn is-primary" id="btn-login">${t.login}</button>
              <button class="btn" id="btn-register">${t.register}</button>
            </div>
          </div>
        </div>
      `;

    const roomsHtml =
      this.openRooms.length === 0
        ? `<div class="badge-muted">${t.noOpenRooms}</div>`
        : this.openRooms
            .map((r) => {
              const lName = r.light ? escapeHtml(r.light.name) : "—";
              const dName = r.dark ? escapeHtml(r.dark.name) : "—";
              const joinable = !r.light || !r.dark;
              return `
                <div class="room-row">
                  <div class="room-meta">
                    <span class="room-players">${lName} vs ${dName}</span>
                    <span class="room-sub">${escapeHtml(r.id)} • ${escapeHtml(r.time_control)} • ${r.status} • ${r.spectators} spec</span>
                  </div>
                  <div class="inline-row">
                    ${
                      joinable
                        ? `<button class="btn is-primary" data-join-room="${escapeHtml(r.id)}">${t.joinRoom}</button>`
                        : ""
                    }
                    <button class="btn" data-watch-room="${escapeHtml(r.id)}">${t.watchRoom}</button>
                  </div>
                </div>
              `;
            })
            .join("");

    const lobbyHtml = this.onlineRoomId
      ? ""
      : `
        <div class="card">
          <div class="card-header">
            <h2 class="card-title">${t.onlineLobby}</h2>
            <button class="btn" id="btn-refresh-lobby">${t.refresh}</button>
          </div>
          <div class="online-stack">
            <div>
              <div class="badge-muted" style="margin-bottom:5px;">${t.quickMatch}</div>
              <div class="inline-row">
                <button class="btn is-primary" data-matchmake="12+5">12+5</button>
                <button class="btn is-primary" data-matchmake="5+3">5+3</button>
                <button class="btn is-primary" data-matchmake="3+2">3+2</button>
              </div>
            </div>
            <div class="inline-row">
              <select id="sel-online-tc">
                ${["12+5", "5+3", "3+2", "unlimited"]
                  .map(
                    (tc) =>
                      `<option value="${tc}" ${this.onlineTimeControl === tc ? "selected" : ""}>${
                        tc === "unlimited" ? t.unlimited : tc
                      }</option>`,
                  )
                  .join("")}
              </select>
              <button class="btn" id="btn-create-private">${t.createPrivate}</button>
            </div>
            <div class="inline-row">
              <input id="inp-room-id" placeholder="${t.roomIdPlaceholder}" />
              <button class="btn is-primary" id="btn-join-id">${t.joinRoom}</button>
              <button class="btn" id="btn-watch-id">${t.watchRoom}</button>
            </div>
            <div class="badge-muted">${t.openRooms}</div>
            <div class="room-list">${roomsHtml}</div>
          </div>
        </div>
      `;

    const lbHtml =
      this.leaderboardPlayers.length === 0
        ? `<div class="badge-muted">${t.noGamesYet}</div>`
        : this.leaderboardPlayers
            .slice(0, 10)
            .map(
              (p, idx) => `
              <div class="table-row">
                <span>#${idx + 1} <strong>${escapeHtml(p.username)}</strong></span>
                <span class="badge-muted">${Math.round(p.rating)} (${p.wins}-${p.losses}-${p.draws})</span>
              </div>
            `,
            )
            .join("");

    const gamesHtml =
      this.recentGames.length === 0
        ? `<div class="badge-muted">${t.noGamesYet}</div>`
        : this.recentGames
            .slice(0, 8)
            .map(
              (g) => `
              <div class="table-row">
                <div class="room-meta">
                  <span class="room-players">${escapeHtml(g.light_name)} vs ${escapeHtml(g.dark_name)} (${escapeHtml(g.result)})</span>
                  <span class="room-sub">${escapeHtml(g.time_control)} • ${g.moves_count} plies • ${escapeHtml(g.reason)}</span>
                </div>
                <button class="btn" data-analyze-game="${escapeHtml(g.id)}">${t.analyzeGame}</button>
              </div>
            `,
            )
            .join("");

    this.onlinePanelEl.innerHTML = `
      ${alertHtml}
      ${inviteHtml}
      ${accountHtml}
      ${lobbyHtml}
      <div class="card">
        <div class="card-header"><h2 class="card-title">${t.leaderboard}</h2></div>
        <div class="compact-table">${lbHtml}</div>
      </div>
      <div class="card">
        <div class="card-header"><h2 class="card-title">${t.recentGames}</h2></div>
        <div class="compact-table">${gamesHtml}</div>
      </div>
    `;

    this.bindOnlinePanelEvents();
  }

  private bindOnlinePanelEvents(): void {
    const panel = this.onlinePanelEl;
    if (!panel) return;

    panel.querySelector("#btn-copy-invite")?.addEventListener("click", () => {
      if (!this.onlineRoomId) return;
      const url = `${window.location.origin}${window.location.pathname}?room=${encodeURIComponent(this.onlineRoomId)}`;
      void navigator.clipboard?.writeText(url);
      this.showToast(this.t().copied);
    });
    panel.querySelector("#btn-inline-leave")?.addEventListener("click", () => {
      this.leaveOnlineRoom(true);
    });
    panel.querySelector("#btn-analyze-finished")?.addEventListener("click", () => {
      if (this.onlineState?.pdn) {
        void this.importPdnText(this.onlineState.pdn, true);
      }
    });

    panel.querySelector<HTMLInputElement>("#inp-guest-name")?.addEventListener("input", (ev) => {
      this.guestName = (ev.target as HTMLInputElement).value.trim() || "Guest";
      localStorage.setItem(LS_GUEST_KEY, this.guestName);
    });

    const getCreds = () => ({
      username: panel.querySelector<HTMLInputElement>("#inp-auth-user")?.value.trim() ?? "",
      password: panel.querySelector<HTMLInputElement>("#inp-auth-pass")?.value ?? "",
    });
    panel.querySelector("#btn-login")?.addEventListener("click", () => {
      const { username, password } = getCreds();
      void this.handleAuthAction("login", username, password);
    });
    panel.querySelector("#btn-register")?.addEventListener("click", () => {
      const { username, password } = getCreds();
      void this.handleAuthAction("register", username, password);
    });
    panel.querySelector("#btn-logout")?.addEventListener("click", () => this.handleLogout());

    panel.querySelector("#btn-refresh-lobby")?.addEventListener("click", () => {
      void this.refreshOnlineLobby();
    });
    panel.querySelector<HTMLSelectElement>("#sel-online-tc")?.addEventListener("change", (ev) => {
      this.onlineTimeControl = (ev.target as HTMLSelectElement).value;
    });
    panel.querySelector("#btn-create-private")?.addEventListener("click", () => {
      void this.startOnlineRoom("private", this.onlineTimeControl);
    });
    panel.querySelectorAll<HTMLButtonElement>("[data-matchmake]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const tc = btn.dataset.matchmake ?? "5+3";
        void this.startOnlineRoom("matchmake", tc);
      });
    });

    const readRoomInput = () =>
      panel.querySelector<HTMLInputElement>("#inp-room-id")?.value.trim() ?? "";
    panel.querySelector("#btn-join-id")?.addEventListener("click", () => {
      this.joinOnlineRoom(readRoomInput(), "player");
    });
    panel.querySelector("#btn-watch-id")?.addEventListener("click", () => {
      this.joinOnlineRoom(readRoomInput(), "spectator");
    });

    panel.querySelectorAll<HTMLButtonElement>("[data-join-room]").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.joinOnlineRoom(btn.dataset.joinRoom ?? "", "player");
      });
    });
    panel.querySelectorAll<HTMLButtonElement>("[data-watch-room]").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.joinOnlineRoom(btn.dataset.watchRoom ?? "", "spectator");
      });
    });
    panel.querySelectorAll<HTMLButtonElement>("[data-analyze-game]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const id = btn.dataset.analyzeGame;
        if (!id) return;
        try {
          const detail = await this.online.gamePdn(id);
          await this.importPdnText(detail.pdn, true);
        } catch (err) {
          this.showToast(err instanceof Error ? err.message : String(err));
        }
      });
    });
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
    if (this.mode === "online" && this.onlineState?.status === "active") {
      this.analysisPanelEl.style.display = "none";
      return;
    }
    this.analysisPanelEl.style.display = "";

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

  private async loadOpeningLine(lineId: string): Promise<void> {
    const line = OPENING_BOOK.find((l) => l.id === lineId);
    if (!line) return;
    await this.engine.clearCache();
    this.stopClock();
    this.forcedOutcome = null;
    this.lastAnalysis = null;

    const startSnap = await this.engine.inspect();
    const timeline: HistoryEntry[] = [
      { fen: startSnap.fen, hash: startSnap.hash, notation: null, lastMove: null },
    ];
    let curFen = startSnap.fen;
    let curSnap = startSnap;
    const hashes = [startSnap.hash];

    for (const moveTok of line.moves) {
      const res = await this.engine.applyMove(curFen, moveTok, hashes);
      timeline.push({
        fen: res.snapshot.fen,
        hash: res.snapshot.hash,
        notation: res.notation,
        lastMove: { from: res.applied.from, to: res.applied.to, path: res.applied.path },
      });
      curFen = res.snapshot.fen;
      curSnap = res.snapshot;
      hashes.push(res.snapshot.hash);
    }

    this.timeline = timeline;
    this.cursor = timeline.length - 1;
    this.currentSnapshot = curSnap;
    this.syncBoard();
    this.refreshPanels();
    void this.triggerAnalysisIfEnabled();
  }

  private renderOpeningPanel(): void {
    if (!this.openingPanelEl || this.mode !== "analysis") return;
    const t = this.t();
    const isTh = this.locale === "th";

    // Only match book when playing from standard starting position
    const startFen = this.timeline[0]?.fen ?? "";
    const startDefault =
      startFen.startsWith("W:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32");
    const playedNotations = startDefault
      ? this.timeline.slice(1, this.cursor + 1).map((e) => e.notation ?? "").filter(Boolean)
      : [];

    const match = matchOpeningHistory(playedNotations);
    const formationTitle = match.matchedFormation
      ? `${isTh ? match.matchedFormation.nameTh : match.matchedFormation.nameEn}`
      : match.activeLine
        ? `${isTh ? match.activeLine.nameTh : match.activeLine.nameEn}`
        : null;

    const noteText = match.activeLine
      ? isTh
        ? match.activeLine.noteTh
        : match.activeLine.noteEn
      : match.matchedFormation
        ? isTh
          ? match.matchedFormation.summaryTh
          : match.matchedFormation.summaryEn
        : playedNotations.length === 0
          ? t.openingStartHint
          : t.outOfBook;

    const formationHtml = `
      <div class="opening-formation-box">
        ${formationTitle ? `<div class="opening-formation-title">${escapeHtml(formationTitle)}</div>` : ""}
        <div class="opening-note">${escapeHtml(noteText)}</div>
      </div>
    `;

    const movesHtml =
      match.nextMoves.length > 0
        ? `<div class="book-move-list">
            ${match.nextMoves
              .map((bm) => {
                const topLine = bm.lines[0];
                const label = topLine
                  ? isTh
                    ? `${topLine.nameTh} • ${topLine.variationTh}`
                    : `${topLine.nameEn} • ${topLine.variationEn}`
                  : "";
                return `
                  <button class="book-move-btn" data-book-move="${escapeHtml(bm.notation)}">
                    <span class="book-move-not">${escapeHtml(bm.notation)}</span>
                    <span class="book-move-sub">${escapeHtml(label)}</span>
                    <span class="badge-muted">${bm.lineCount} ${t.openingLinesCount}</span>
                  </button>
                `;
              })
              .join("")}
          </div>`
        : "";

    const chipsHtml = `
      <div class="formation-chips">
        ${FORMATIONS.map((f) => {
          const firstLine = OPENING_BOOK.find((l) => l.formationId === f.id);
          const isActive = match.matchedFormation?.id === f.id;
          return `<button class="formation-chip ${isActive ? "is-active" : ""}" data-formation-line="${escapeHtml(
            firstLine?.id ?? "",
          )}">${escapeHtml(isTh ? f.nameTh : f.nameEn)}</button>`;
        }).join("")}
      </div>
    `;

    this.openingPanelEl.innerHTML = `
      <div class="card-header">
        <h2 class="card-title">${t.openingExplorer}</h2>
        <span class="badge-muted">${match.matchingLines.length} ${t.openingLinesCount}</span>
      </div>
      ${formationHtml}
      ${movesHtml}
      ${chipsHtml}
    `;

    this.openingPanelEl.querySelectorAll<HTMLButtonElement>("[data-book-move]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const tok = btn.dataset.bookMove;
        if (tok && this.canHumanMoveNow()) {
          void this.commitNotation(tok);
        }
      });
    });

    this.openingPanelEl.querySelectorAll<HTMLButtonElement>("[data-formation-line]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const lineId = btn.dataset.formationLine;
        if (lineId) void this.loadOpeningLine(lineId);
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
