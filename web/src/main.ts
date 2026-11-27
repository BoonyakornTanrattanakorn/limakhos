import "./styles/app.css";
import "./styles/board.css";
import { MakhosBoard, type BoardArrow } from "./board/MakhosBoard";
import { AnalysisCancelledError, MakhosEngineClient } from "./engine/client";
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
import { BASICS_LESSONS, type BasicsLesson, type BasicsLessonStep } from "./study/basics";
import { FORMATIONS, OPENING_BOOK, matchOpeningHistory } from "./study/openings";
import { PRACTICE_MODULES, type PracticeDrill, type PracticeModule } from "./study/practice";
import { KON_MAKHOS_PUZZLES, type KonMakhosPuzzle } from "./study/puzzles";
import { evalToLightPercent, formatClockMs, formatEvalLight } from "./ui/eval";
import { ICONS } from "./ui/icons";
import {
  BOARD_THEMES,
  applyThemeAttributes,
  loadUiPrefs,
  saveUiPrefs,
  type BoardTheme,
  type UiPrefs,
  type UiTheme,
} from "./ui/prefs";

type PlayMode = "analysis" | "ai" | "local" | "online" | "basics" | "puzzles" | "practice";
type NavMenuGroup = "play" | "learn" | "tools";
type ToolMenuItem = "analysis" | "opening" | "import";

interface TimeControlOption {
  id: string;
  label: string;
  initialMs: number;
  incrementMs: number;
}

const TIME_CONTROLS: TimeControlOption[] = [
  { id: "unlimited", label: "∞", initialMs: 0, incrementMs: 0 },
  { id: "12+5", label: "12+5", initialMs: 12 * 60_000, incrementMs: 5_000 },
  { id: "5+3", label: "5+3", initialMs: 5 * 60_000, incrementMs: 3_000 },
  { id: "3+2", label: "3+2", initialMs: 3 * 60_000, incrementMs: 2_000 },
];

const LS_TOKEN_KEY = "limakhos_auth_token";
const LS_GUEST_KEY = "limakhos_guest_name";
const LS_STUDY_PROGRESS_KEY = "limakhos_study_progress_v1";
const BLACK_FIRST_START_FEN = "B:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32";

/** Discrete time-limit steps for the analysis slider; 0 at the right end means unlimited (∞). */
const ANALYSIS_TIME_STEPS_MS = [
  250, 500, 1000, 1500, 2000, 3000, 5000, 8000, 12000, 20000, 30000, 60000, 0,
];

function formatAnalysisTime(ms: number): string {
  if (ms <= 0) return "∞";
  if (ms < 1000) return `${ms}ms`;
  const secs = ms / 1000;
  return Number.isInteger(secs) ? `${secs}s` : `${secs.toFixed(1)}s`;
}

function timeMsToSliderIndex(ms: number): number {
  const exact = ANALYSIS_TIME_STEPS_MS.indexOf(ms);
  if (exact !== -1) return exact;
  if (ms <= 0) return ANALYSIS_TIME_STEPS_MS.length - 1;
  let bestIdx = 0;
  let bestDiff = Number.POSITIVE_INFINITY;
  for (let i = 0; i < ANALYSIS_TIME_STEPS_MS.length; i++) {
    const step = ANALYSIS_TIME_STEPS_MS[i];
    if (step <= 0) continue;
    const diff = Math.abs(step - ms);
    if (diff < bestDiff) {
      bestDiff = diff;
      bestIdx = i;
    }
  }
  return bestIdx;
}

function formatElapsedMs(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(1)} s`;
}

function formatCompactCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 10_000) return `${(n / 1_000).toFixed(1)}k`;
  return n.toLocaleString();
}

interface StudyProgress {
  puzzleRating: number;
  solvedPuzzles: string[];
  completedDrills: string[];
  completedBasics: string[];
}

function loadStudyProgress(): StudyProgress {
  try {
    const raw = localStorage.getItem(LS_STUDY_PROGRESS_KEY);
    if (!raw) return { puzzleRating: 1200, solvedPuzzles: [], completedDrills: [], completedBasics: [] };
    const parsed = JSON.parse(raw) as Partial<StudyProgress>;
    return {
      puzzleRating: typeof parsed.puzzleRating === "number" ? parsed.puzzleRating : 1200,
      solvedPuzzles: Array.isArray(parsed.solvedPuzzles) ? parsed.solvedPuzzles : [],
      completedDrills: Array.isArray(parsed.completedDrills) ? parsed.completedDrills : [],
      completedBasics: Array.isArray(parsed.completedBasics) ? parsed.completedBasics : [],
    };
  } catch {
    return { puzzleRating: 1200, solvedPuzzles: [], completedDrills: [], completedBasics: [] };
  }
}

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

  private prefs: UiPrefs = loadUiPrefs();
  private locale: Locale = this.prefs.locale;
  private mode: PlayMode = "analysis";
  private humanColor: Color = "light";
  private aiLevel = 5;
  private timeControlId = "unlimited";
  private orientation: Color = "light";
  private showNumbers = this.prefs.showNumbers;
  private showMainDiagonal = this.prefs.showMainDiagonal;

  // UI-only view states
  private settingsOpen = false;
  private openNavMenu: NavMenuGroup | null = null;
  private navOpenedByHover = false;
  private activeToolItem: ToolMenuItem = "analysis";
  private underboardTab: "opening" | "fen" = "opening";
  private onlineSubTab: "lobby" | "leaderboard" | "recent" = "lobby";

  private analysisEnabled = true;
  private analysisThreads = 1;
  private analysisDepth = 10;
  private analysisTimeMs = 1500;
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
  private resignConfirming = false;
  private resignConfirmTimer: number | null = null;
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

  // Puzzles, Basics & Practice state
  private studyProgress: StudyProgress = loadStudyProgress();
  private activeBasicsLessonIdx = 0;
  private activeBasicsStepIdx = 0;
  private basicsStatus: "trying" | "wrong" | "completed" = "trying";
  private basicsShowHint = false;
  private basicsHintArrow: BoardArrow | null = null;
  private basicsRunSeq = 0;

  private activePuzzleIdx = 0;
  private puzzleStep = 0;
  private puzzleStatus: "solving" | "wrong" | "solved" | "revealed" = "solving";
  private puzzleShowHint = false;
  private puzzleHintArrow: BoardArrow | null = null;
  private puzzleFailedAttempt = false;
  private puzzleRunSeq = 0;

  private activeModuleIdx = 0;
  private activeDrillIdx = 0;
  private practiceLiveEval = false;
  private practiceHintArrow: BoardArrow | null = null;

  // DOM refs
  private root: HTMLElement;
  private settingsWrapEl: HTMLElement | null = null;
  private settingsPopoverEl: HTMLElement | null = null;
  private settingsBtnEl: HTMLButtonElement | null = null;
  private userChipEl: HTMLButtonElement | null = null;
  private boardStageEl!: HTMLElement;
  private boardMount!: HTMLElement;
  private evalFill!: HTMLElement;
  private evalTopLabel!: HTMLElement;
  private evalBottomLabel!: HTMLElement;
  private topPlayerEl!: HTMLElement;
  private bottomPlayerEl!: HTMLElement;
  private statusEl!: HTMLElement;
  private onlinePanelEl: HTMLElement | null = null;
  private studyPanelEl: HTMLElement | null = null;
  private analysisPanelEl!: HTMLElement;
  private openingPanelEl: HTMLElement | null = null;
  private fenPanelEl!: HTMLElement;
  private movesPanelEl!: HTMLElement;
  private ioTextarea!: HTMLTextAreaElement;
  private toastEl!: HTMLElement;

  constructor(root: HTMLElement) {
    this.root = root;
    applyThemeAttributes(this.prefs);

    const urlRoom = new URLSearchParams(window.location.search).get("room")?.trim();
    if (urlRoom) {
      this.mode = "online";
      this.analysisEnabled = false;
    } else if (this.prefs.blackFirst) {
      this.orientation = "dark";
    }

    this.renderShell();

    document.addEventListener("click", (ev) => {
      const target = ev.target;
      if (this.resignConfirming) {
        if (!(target instanceof Element && target.closest("#btn-resign"))) {
          this.clearResignConfirm();
        }
      }
      if (this.openNavMenu !== null) {
        if (!(target instanceof Element && target.closest(".site-nav"))) {
          this.openNavMenu = null;
          this.navOpenedByHover = false;
          this.syncNavMenus();
        }
      }
      if (!this.settingsOpen) return;
      if (target instanceof Node && this.settingsWrapEl?.contains(target)) return;
      if (target instanceof Element && target.closest(".settings-wrap")) return;
      this.settingsOpen = false;
      this.syncSettingsPopover();
    });
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

  private updatePrefs(partial: Partial<UiPrefs>): void {
    this.prefs = { ...this.prefs, ...partial };
    this.locale = this.prefs.locale;
    this.showNumbers = this.prefs.showNumbers;
    this.showMainDiagonal = this.prefs.showMainDiagonal;
    saveUiPrefs(this.prefs);
    applyThemeAttributes(this.prefs);
  }

  private async initOnlineFromUrl(roomId: string): Promise<void> {
    await this.startNewGame();
    await this.refreshOnlineLobby();
    this.joinOnlineRoom(roomId, "player");
  }

  private modeTitle(mode: PlayMode): string {
    const t = this.t();
    switch (mode) {
      case "analysis":
        return t.modeAnalysis;
      case "ai":
        return t.modeAi;
      case "local":
        return t.modeLocal;
      case "basics":
        return t.modeBasics;
      case "puzzles":
        return t.modePuzzles;
      case "practice":
        return t.modePractice;
      case "online":
        return t.modeOnline;
    }
  }

  private modeIcon(mode: PlayMode): string {
    return ICONS[mode];
  }

  private shouldShowEvalBar(): boolean {
    if (!this.analysisEnabled) return false;
    if (this.mode === "puzzles" || this.mode === "basics") return false;
    if (this.mode === "online" && this.onlineState?.status === "active") return false;
    if (this.mode === "practice" && !this.practiceLiveEval) return false;
    return true;
  }

  private syncNavMenus(): void {
    this.root.querySelectorAll<HTMLElement>("[data-nav-group]").forEach((groupEl) => {
      const groupId = groupEl.dataset.navGroup as NavMenuGroup;
      const isOpen = this.openNavMenu === groupId;
      groupEl.classList.toggle("is-open", isOpen);
      const trigger = groupEl.querySelector<HTMLButtonElement>("[data-nav-trigger]");
      if (trigger) {
        trigger.setAttribute("aria-expanded", String(isOpen));
      }
    });
    this.root.querySelectorAll<HTMLButtonElement>("[data-nav-tool]").forEach((btn) => {
      const tool = btn.dataset.navTool as ToolMenuItem;
      btn.classList.toggle("is-active", this.mode === "analysis" && this.activeToolItem === tool);
    });
  }

  private async selectToolItem(tool: ToolMenuItem): Promise<void> {
    this.openNavMenu = null;
    this.navOpenedByHover = false;
    this.activeToolItem = tool;
    const targetTab: "opening" | "fen" = tool === "import" ? "fen" : "opening";

    if (this.mode !== "analysis") {
      this.leaveOnlineRoom(false);
      this.clearResignConfirm(false);
      this.cancelAnalysis();
      this.mode = "analysis";
      this.settingsOpen = false;
      this.analysisEnabled = true;
      this.orientation = this.prefs.blackFirst ? "dark" : "light";
      this.underboardTab = targetTab;
      this.renderShell();
      await this.startNewGame();
    } else {
      this.syncNavMenus();
      this.underboardTab = targetTab;
      this.syncUnderboardTabs();
    }

    if (tool === "opening") {
      this.openingPanelEl?.scrollIntoView({ behavior: "smooth", block: "nearest" });
    } else if (tool === "import") {
      this.ioTextarea?.scrollIntoView({ behavior: "smooth", block: "nearest" });
      this.ioTextarea?.focus();
    }
  }

  private switchMode(nextMode: PlayMode): void {
    this.openNavMenu = null;
    this.navOpenedByHover = false;
    if (nextMode === this.mode) {
      this.syncNavMenus();
      if (nextMode === "online") {
        void this.refreshOnlineLobby();
      }
      return;
    }
    this.leaveOnlineRoom(false);
    this.clearResignConfirm(false);
    this.cancelAnalysis();
    this.mode = nextMode;
    this.settingsOpen = false;
    this.analysisEnabled =
      nextMode === "analysis" || (nextMode === "practice" && this.practiceLiveEval);
    if (nextMode === "ai") {
      this.orientation = this.humanColor;
    } else if (nextMode === "analysis" || nextMode === "local") {
      this.orientation = this.prefs.blackFirst ? "dark" : "light";
    }
    if (nextMode === "analysis") {
      this.activeToolItem = "analysis";
      this.underboardTab = "opening";
    }
    this.renderShell();
    if (nextMode === "online") {
      void this.startNewGame();
      void this.refreshOnlineLobby();
    } else if (nextMode === "basics") {
      void this.loadBasicsStep(this.activeBasicsLessonIdx, this.activeBasicsStepIdx);
    } else if (nextMode === "puzzles") {
      void this.loadPuzzle(this.activePuzzleIdx);
    } else if (nextMode === "practice") {
      void this.loadPracticeDrill(this.activeModuleIdx, this.activeDrillIdx);
    } else {
      void this.startNewGame();
    }
  }

  private renderShell(): void {
    const t = this.t();
    const preservedIoValue = this.ioTextarea?.value ?? "";
    const boardLabelMap: Record<BoardTheme, string> = {
      brown: t.boardBrown,
      blue: t.boardBlue,
      green: t.boardGreen,
      walnut: t.boardWalnut,
    };

    const isPlayGroup = this.mode === "ai" || this.mode === "online" || this.mode === "local";
    const isLearnGroup = this.mode === "basics" || this.mode === "practice" || this.mode === "puzzles";
    const isToolsGroup = this.mode === "analysis";

    const navHtml = `
      <div class="nav-group ${this.openNavMenu === "play" ? "is-open" : ""}" data-nav-group="play">
        <button
          type="button"
          class="site-nav-btn ${isPlayGroup ? "is-active" : ""}"
          data-nav-trigger="play"
          aria-expanded="${this.openNavMenu === "play"}"
          aria-haspopup="true"
        >
          ${ICONS.play}
          <span>${escapeHtml(t.navPlay)}</span>
          ${ICONS.chevronDown}
        </button>
        <div class="nav-dropdown" role="menu">
          <button type="button" role="menuitem" class="nav-dropdown-item ${this.mode === "ai" ? "is-active" : ""}" data-nav-mode="ai">
            ${ICONS.ai}
            <span>${escapeHtml(t.modeAi)}</span>
          </button>
          <button type="button" role="menuitem" class="nav-dropdown-item ${this.mode === "online" ? "is-active" : ""}" data-nav-mode="online">
            ${ICONS.online}
            <span>${escapeHtml(t.modeOnline)}</span>
          </button>
          <button type="button" role="menuitem" class="nav-dropdown-item ${this.mode === "local" ? "is-active" : ""}" data-nav-mode="local">
            ${ICONS.local}
            <span>${escapeHtml(t.modeLocal)}</span>
          </button>
        </div>
      </div>

      <div class="nav-group ${this.openNavMenu === "learn" ? "is-open" : ""}" data-nav-group="learn">
        <button
          type="button"
          class="site-nav-btn ${isLearnGroup ? "is-active" : ""}"
          data-nav-trigger="learn"
          aria-expanded="${this.openNavMenu === "learn"}"
          aria-haspopup="true"
        >
          ${ICONS.basics}
          <span>${escapeHtml(t.navLearn)}</span>
          ${ICONS.chevronDown}
        </button>
        <div class="nav-dropdown" role="menu">
          <button type="button" role="menuitem" class="nav-dropdown-item ${this.mode === "basics" ? "is-active" : ""}" data-nav-mode="basics">
            ${ICONS.basics}
            <span>${escapeHtml(t.modeBasics)}</span>
          </button>
          <button type="button" role="menuitem" class="nav-dropdown-item ${this.mode === "practice" ? "is-active" : ""}" data-nav-mode="practice">
            ${ICONS.practice}
            <span>${escapeHtml(t.modePractice)}</span>
          </button>
          <button type="button" role="menuitem" class="nav-dropdown-item ${this.mode === "puzzles" ? "is-active" : ""}" data-nav-mode="puzzles">
            ${ICONS.puzzles}
            <span>${escapeHtml(t.modePuzzles)}</span>
          </button>
        </div>
      </div>

      <div class="nav-group ${this.openNavMenu === "tools" ? "is-open" : ""}" data-nav-group="tools">
        <button
          type="button"
          class="site-nav-btn ${isToolsGroup ? "is-active" : ""}"
          data-nav-trigger="tools"
          aria-expanded="${this.openNavMenu === "tools"}"
          aria-haspopup="true"
        >
          ${ICONS.tools}
          <span>${escapeHtml(t.navTools)}</span>
          ${ICONS.chevronDown}
        </button>
        <div class="nav-dropdown" role="menu">
          <button type="button" role="menuitem" class="nav-dropdown-item ${isToolsGroup && this.activeToolItem === "analysis" ? "is-active" : ""}" data-nav-tool="analysis">
            ${ICONS.analysis}
            <span>${escapeHtml(t.toolAnalysisBoard)}</span>
          </button>
          <button type="button" role="menuitem" class="nav-dropdown-item ${isToolsGroup && this.activeToolItem === "opening" ? "is-active" : ""}" data-nav-tool="opening">
            ${ICONS.book}
            <span>${escapeHtml(t.toolOpeningExplorer)}</span>
          </button>
          <button type="button" role="menuitem" class="nav-dropdown-item ${isToolsGroup && this.activeToolItem === "import" ? "is-active" : ""}" data-nav-tool="import">
            ${ICONS.import}
            <span>${escapeHtml(t.toolImportGame)}</span>
          </button>
        </div>
      </div>
    `;

    const leftModeHtml = this.renderLeftModeHtml();
    const showEval = this.shouldShowEvalBar();
    const showOpeningTab = this.mode === "analysis";
    const activeUnderboard = showOpeningTab ? this.underboardTab : "fen";

    this.root.innerHTML = `
      <header class="site-header">
        <div class="site-header-inner">
          <div class="site-brand" aria-label="${t.appTitle}">
            ${ICONS.logo}
            <span class="site-brand-title">limakhos</span>
            <span class="site-brand-sub">8</span>
          </div>

          <nav class="site-nav" aria-label="Modes">
            ${navHtml}
          </nav>

          <div class="site-utils">
            <button type="button" class="user-chip" id="btn-user-chip" title="${t.onlineAccount}">
              ${ICONS.user}
              <span class="user-chip-name">${escapeHtml(this.authUser?.username ?? this.guestName)}</span>
              ${
                this.authUser
                  ? `<span class="user-chip-rating">${Math.round(this.authUser.rating)}</span>`
                  : ""
              }
            </button>

            <button type="button" class="locale-btn" id="btn-locale" title="Language">
              ${this.locale === "th" ? "EN" : "ไทย"}
            </button>

            <div class="settings-wrap" id="settings-wrap">
              <button
                type="button"
                class="icon-btn ${this.settingsOpen ? "is-active" : ""}"
                id="btn-settings"
                aria-expanded="${this.settingsOpen}"
                aria-haspopup="true"
                aria-label="${t.settings}"
                title="${t.settings}"
              >
                ${ICONS.settings}
              </button>

              <div class="settings-popover" id="settings-popover" ${this.settingsOpen ? "" : "hidden"}>
                <div>
                  <div class="settings-section-title">${t.uiTheme}</div>
                  <div class="segmented-group">
                    <button
                      type="button"
                      class="seg-btn ${this.prefs.uiTheme === "dark" ? "is-active" : ""}"
                      data-ui-theme="dark"
                    >
                      ${ICONS.moon}
                      <span>${t.themeDark}</span>
                    </button>
                    <button
                      type="button"
                      class="seg-btn ${this.prefs.uiTheme === "light" ? "is-active" : ""}"
                      data-ui-theme="light"
                    >
                      ${ICONS.sun}
                      <span>${t.themeLight}</span>
                    </button>
                  </div>
                </div>

                <div>
                  <div class="settings-section-title">${t.boardTheme}</div>
                  <div class="swatch-grid">
                    ${BOARD_THEMES.map(
                      (bt) => `
                        <button
                          type="button"
                          class="swatch-btn ${this.prefs.boardTheme === bt.id ? "is-active" : ""}"
                          data-board-theme="${bt.id}"
                        >
                          <span class="swatch-preview">
                            <span style="background:${bt.lightHex}"></span>
                            <span style="background:${bt.darkHex}"></span>
                            <span style="background:${bt.darkHex}"></span>
                            <span style="background:${bt.lightHex}"></span>
                          </span>
                          <span>${escapeHtml(boardLabelMap[bt.id])}</span>
                        </button>
                      `,
                    ).join("")}
                  </div>
                </div>

                <div>
                  <div class="settings-section-title">${t.boardOptions}</div>
                  <div class="theme-toggle-row">
                    <span>${t.numbersToggle}</span>
                    <button
                      type="button"
                      role="switch"
                      aria-checked="${this.showNumbers}"
                      class="li-switch ${this.showNumbers ? "is-on" : ""}"
                      id="btn-nums"
                    ></button>
                  </div>
                  <div class="theme-toggle-row">
                    <span>${t.mainDiagToggle}</span>
                    <button
                      type="button"
                      role="switch"
                      aria-checked="${this.showMainDiagonal}"
                      class="li-switch ${this.showMainDiagonal ? "is-on" : ""}"
                      id="btn-diag"
                    ></button>
                  </div>
                  <div class="theme-toggle-row">
                    <span>${t.blackFirstToggle}</span>
                    <button
                      type="button"
                      role="switch"
                      aria-checked="${this.prefs.blackFirst}"
                      class="li-switch ${this.prefs.blackFirst ? "is-on" : ""}"
                      id="btn-black-first"
                    ></button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </header>

      <main class="lichess-main">
        <aside class="col-left">
          <div class="li-box">
            <div class="li-box-header">
              <h2 class="li-box-title">${this.modeIcon(this.mode)} <span>${this.modeTitle(this.mode)}</span></h2>
              <span class="li-badge">TCA 2025</span>
            </div>
            <div class="li-box-body">
              <div id="status-box"></div>
            </div>
          </div>
          ${leftModeHtml}
        </aside>

        <section class="col-center">
          <div class="board-area">
            <div class="board-stage ${showEval ? "" : "no-eval"}" id="board-stage">
              <div class="eval-bar" title="${escapeHtml(t.evalBarLabel)}">
                <span class="eval-bar-label is-top" id="eval-top">0.0</span>
                <div class="eval-bar-fill" id="eval-fill" style="height: 50%"></div>
                <span class="eval-bar-label is-bottom" id="eval-bottom"></span>
              </div>
              <div id="board-mount"></div>
            </div>
          </div>

          <div class="underboard li-box">
            <div class="li-tabs" role="tablist">
              ${
                showOpeningTab
                  ? `<button
                      type="button"
                      role="tab"
                      aria-selected="${activeUnderboard === "opening"}"
                      class="li-tab ${activeUnderboard === "opening" ? "is-active" : ""}"
                      data-underboard-tab="opening"
                    >
                      ${ICONS.book}
                      <span>${t.tabOpening}</span>
                    </button>`
                  : ""
              }
              <button
                type="button"
                role="tab"
                aria-selected="${activeUnderboard === "fen"}"
                class="li-tab ${activeUnderboard === "fen" ? "is-active" : ""}"
                data-underboard-tab="fen"
              >
                ${ICONS.code}
                <span>${t.tabFenPdn}</span>
              </button>
              <span class="toast-msg" id="toast-msg">${escapeHtml(this.toastMessage)}</span>
            </div>
            <div class="li-box-body">
              ${
                showOpeningTab
                  ? `<div id="opening-card" ${activeUnderboard === "opening" ? "" : "hidden"}></div>`
                  : ""
              }
              <div id="fen-panel" ${activeUnderboard === "fen" ? "" : "hidden"}>
                <div class="io-box">
                  <textarea
                    id="io-text"
                    class="li-textarea"
                    placeholder="W:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32  or  PDN..."
                  >${escapeHtml(preservedIoValue)}</textarea>
                  <div class="io-actions">
                    <button type="button" class="li-btn is-sm" id="btn-copy-fen">${ICONS.copy} <span>${t.copyFen}</span></button>
                    <button type="button" class="li-btn is-sm" id="btn-load-fen" ${this.mode === "online" ? "disabled" : ""}>${t.loadFen}</button>
                    <button type="button" class="li-btn is-sm" id="btn-copy-pdn">${ICONS.copy} <span>${t.exportPdn}</span></button>
                    <button type="button" class="li-btn is-sm" id="btn-import-pdn" ${this.mode === "online" ? "disabled" : ""}>${t.importPdn}</button>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <aside class="col-right tools-panel">
          <div id="top-player" class="player-strip is-top"></div>

          <div class="li-box tools-box">
            <div id="analysis-card" class="engine-box"></div>
            <div id="moves-card"></div>

            <div class="replay-bar">
              <button type="button" id="nav-start" title="Home" aria-label="First move" ${this.cursor === 0 ? "disabled" : ""}>${ICONS.first}</button>
              <button type="button" id="nav-prev" title="←" aria-label="Previous move" ${this.cursor === 0 ? "disabled" : ""}>${ICONS.prev}</button>
              <button type="button" id="nav-next" title="→" aria-label="Next move" ${this.cursor >= this.timeline.length - 1 ? "disabled" : ""}>${ICONS.next}</button>
              <button type="button" id="nav-end" title="End" aria-label="Latest move" ${this.cursor >= this.timeline.length - 1 ? "disabled" : ""}>${ICONS.last}</button>
              <button type="button" id="btn-flip" title="${escapeHtml(t.flipBoard)} (F)" aria-label="${escapeHtml(t.flipBoard)}">${ICONS.flip}</button>
            </div>

            <div class="action-bar">
              ${
                this.mode === "online" && this.onlineRoomId
                  ? `<button type="button" class="li-action-btn is-leave" id="btn-leave-room">${ICONS.leave}<span>${escapeHtml(t.leaveRoom)}</span></button>`
                  : `<button type="button" class="li-action-btn is-new" id="btn-new">${ICONS.newGame}<span>${escapeHtml(t.newGame)}</span></button>`
              }
              <button type="button" class="li-action-btn is-undo" id="btn-undo" ${
                this.canUndoNow() ? "" : "disabled"
              }>${ICONS.undo}<span>${escapeHtml(t.undo)}</span></button>
              <button type="button" class="li-action-btn is-resign" id="btn-resign" ${
                this.canResignNow() ? "" : "disabled"
              }>${ICONS.resign}<span>${escapeHtml(t.resign)}</span></button>
            </div>
          </div>

          <div id="bottom-player" class="player-strip is-bottom"></div>
        </aside>
      </main>
    `;

    this.settingsWrapEl = this.root.querySelector("#settings-wrap");
    this.settingsPopoverEl = this.root.querySelector("#settings-popover");
    this.settingsBtnEl = this.root.querySelector("#btn-settings");
    this.userChipEl = this.root.querySelector("#btn-user-chip");
    this.boardStageEl = this.root.querySelector("#board-stage")!;
    this.boardMount = this.root.querySelector("#board-mount")!;
    this.evalFill = this.root.querySelector("#eval-fill")!;
    this.evalTopLabel = this.root.querySelector("#eval-top")!;
    this.evalBottomLabel = this.root.querySelector("#eval-bottom")!;
    this.topPlayerEl = this.root.querySelector("#top-player")!;
    this.bottomPlayerEl = this.root.querySelector("#bottom-player")!;
    this.statusEl = this.root.querySelector("#status-box")!;
    this.onlinePanelEl = this.root.querySelector("#online-panel");
    this.studyPanelEl = this.root.querySelector("#study-panel");
    this.analysisPanelEl = this.root.querySelector("#analysis-card")!;
    this.openingPanelEl = this.root.querySelector("#opening-card");
    this.fenPanelEl = this.root.querySelector("#fen-panel")!;
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

  private renderLeftModeHtml(): string {
    const t = this.t();

    if (this.mode === "ai" || this.mode === "local") {
      return `
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${t.settings}</h2>
          </div>
          <div class="li-box-body" style="display:flex;flex-direction:column;gap:10px;">
            ${
              this.mode === "ai"
                ? `
                  <div class="field">
                    <span class="field-label">${t.playAs}</span>
                    <div class="segmented-group" id="ai-side-group">
                      <button type="button" class="seg-btn ${this.humanColor === "light" ? "is-active" : ""}" data-side="light">
                        <span class="color-dot is-light"></span>
                        <span>${t.lightShort}</span>
                      </button>
                      <button type="button" class="seg-btn ${this.humanColor === "dark" ? "is-active" : ""}" data-side="dark">
                        <span class="color-dot is-dark"></span>
                        <span>${t.darkShort}</span>
                      </button>
                    </div>
                  </div>
                  <div class="field">
                    <span class="field-label">${t.aiLevel}</span>
                    <div class="level-grid" id="ai-level-grid">
                      ${[1, 2, 3, 4, 5, 6, 7, 8]
                        .map(
                          (lvl) => `
                            <button type="button" class="level-btn ${this.aiLevel === lvl ? "is-active" : ""}" data-level="${lvl}">
                              ${lvl}
                            </button>
                          `,
                        )
                        .join("")}
                    </div>
                  </div>
                `
                : ""
            }
            <div class="field">
              <span class="field-label">${t.firstMover}</span>
              <div class="segmented-group" id="first-mover-group">
                <button type="button" class="seg-btn ${!this.prefs.blackFirst ? "is-active" : ""}" data-first-mover="light">
                  <span class="color-dot is-light"></span>
                  <span>${t.lightShort}</span>
                </button>
                <button type="button" class="seg-btn ${this.prefs.blackFirst ? "is-active" : ""}" data-first-mover="dark">
                  <span class="color-dot is-dark"></span>
                  <span>${t.darkShort}</span>
                </button>
              </div>
            </div>
            <div class="field">
              <span class="field-label">${t.timeControl}</span>
              <div class="segmented-group" id="tc-group">
                ${TIME_CONTROLS.map(
                  (tc) => `
                    <button type="button" class="seg-btn ${this.timeControlId === tc.id ? "is-active" : ""}" data-tc="${tc.id}">
                      ${escapeHtml(tc.label)}
                    </button>
                  `,
                ).join("")}
              </div>
            </div>
          </div>
        </div>
      `;
    }

    if (this.mode === "basics" || this.mode === "puzzles" || this.mode === "practice") {
      return `<div id="study-panel" class="online-stack"></div>`;
    }

    if (this.mode === "online") {
      return `<div id="online-panel" class="online-stack"></div>`;
    }

    // analysis mode: compact info/tips box
    return `
      <div class="li-box">
        <div class="li-box-header">
          <h2 class="li-box-title">${ICONS.spark} <span>${t.subtitle}</span></h2>
        </div>
        <div class="li-box-body" style="display:flex;flex-direction:column;gap:6px;font-size:0.78rem;color:var(--text-muted);">
          <div>${escapeHtml(t.rightClickHint)}</div>
          <div>${escapeHtml(t.shortcutsHint)}</div>
        </div>
      </div>
    `;
  }

  private syncSettingsPopover(): void {
    if (this.settingsPopoverEl) {
      this.settingsPopoverEl.hidden = !this.settingsOpen;
    }
    if (this.settingsBtnEl) {
      this.settingsBtnEl.classList.toggle("is-active", this.settingsOpen);
      this.settingsBtnEl.setAttribute("aria-expanded", String(this.settingsOpen));
    }
    this.root.querySelectorAll<HTMLButtonElement>("[data-ui-theme]").forEach((btn) => {
      btn.classList.toggle("is-active", btn.dataset.uiTheme === this.prefs.uiTheme);
    });
    this.root.querySelectorAll<HTMLButtonElement>("[data-board-theme]").forEach((btn) => {
      btn.classList.toggle("is-active", btn.dataset.boardTheme === this.prefs.boardTheme);
    });
    const numsBtn = this.root.querySelector<HTMLButtonElement>("#btn-nums");
    if (numsBtn) {
      numsBtn.classList.toggle("is-on", this.showNumbers);
      numsBtn.setAttribute("aria-checked", String(this.showNumbers));
    }
    const diagBtn = this.root.querySelector<HTMLButtonElement>("#btn-diag");
    if (diagBtn) {
      diagBtn.classList.toggle("is-on", this.showMainDiagonal);
      diagBtn.setAttribute("aria-checked", String(this.showMainDiagonal));
    }
    const blackFirstBtn = this.root.querySelector<HTMLButtonElement>("#btn-black-first");
    if (blackFirstBtn) {
      blackFirstBtn.classList.toggle("is-on", this.prefs.blackFirst);
      blackFirstBtn.setAttribute("aria-checked", String(this.prefs.blackFirst));
    }
    this.root.querySelectorAll<HTMLButtonElement>("[data-first-mover]").forEach((btn) => {
      const wantDark = btn.dataset.firstMover === "dark";
      btn.classList.toggle("is-active", wantDark === this.prefs.blackFirst);
    });
  }

  private setBlackFirst(enabled: boolean): void {
    this.updatePrefs({ blackFirst: enabled });
    this.syncSettingsPopover();
    if (this.mode === "analysis" || this.mode === "local") {
      this.orientation = enabled ? "dark" : "light";
    }
    if (this.mode === "analysis" || this.mode === "ai" || this.mode === "local") {
      void this.startNewGame();
    }
  }

  private renderUserChip(): void {
    if (!this.userChipEl) return;
    this.userChipEl.innerHTML = `
      ${ICONS.user}
      <span class="user-chip-name">${escapeHtml(this.authUser?.username ?? this.guestName)}</span>
      ${
        this.authUser
          ? `<span class="user-chip-rating">${Math.round(this.authUser.rating)}</span>`
          : ""
      }
    `;
  }

  private syncUnderboardTabs(): void {
    const showOpening = this.mode === "analysis";
    const activeTab = showOpening ? this.underboardTab : "fen";
    this.root.querySelectorAll<HTMLButtonElement>("[data-underboard-tab]").forEach((btn) => {
      const isActive = btn.dataset.underboardTab === activeTab;
      btn.classList.toggle("is-active", isActive);
      btn.setAttribute("aria-selected", String(isActive));
    });
    if (this.openingPanelEl) {
      this.openingPanelEl.hidden = !(showOpening && activeTab === "opening");
    }
    if (this.fenPanelEl) {
      this.fenPanelEl.hidden = showOpening && activeTab === "opening";
    }
  }

  private bindControls(): void {
    this.root.querySelectorAll<HTMLElement>("[data-nav-group]").forEach((groupEl) => {
      const groupId = groupEl.dataset.navGroup as NavMenuGroup;
      groupEl.addEventListener("pointerenter", (ev) => {
        if (ev.pointerType !== "mouse") return;
        if (this.settingsOpen) {
          this.settingsOpen = false;
          this.syncSettingsPopover();
        }
        this.openNavMenu = groupId;
        this.navOpenedByHover = true;
        this.syncNavMenus();
      });
      groupEl.addEventListener("pointerleave", (ev) => {
        if (ev.pointerType !== "mouse") return;
        if (this.openNavMenu === groupId) {
          this.openNavMenu = null;
          this.navOpenedByHover = false;
          this.syncNavMenus();
        }
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-nav-trigger]").forEach((btn) => {
      btn.addEventListener("click", (ev) => {
        ev.stopPropagation();
        const groupId = btn.dataset.navTrigger as NavMenuGroup;
        if (this.settingsOpen) {
          this.settingsOpen = false;
          this.syncSettingsPopover();
        }
        if (this.openNavMenu === groupId) {
          if (this.navOpenedByHover) {
            this.navOpenedByHover = false;
          } else {
            this.openNavMenu = null;
          }
        } else {
          this.openNavMenu = groupId;
          this.navOpenedByHover = false;
        }
        this.syncNavMenus();
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-nav-mode]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const nextMode = btn.dataset.navMode as PlayMode;
        this.switchMode(nextMode);
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-nav-tool]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const tool = btn.dataset.navTool as ToolMenuItem;
        void this.selectToolItem(tool);
      });
    });

    this.userChipEl?.addEventListener("click", () => {
      this.switchMode("online");
    });

    this.root.querySelector("#btn-locale")?.addEventListener("click", () => {
      const nextLocale: Locale = this.locale === "th" ? "en" : "th";
      this.updatePrefs({ locale: nextLocale });
      this.renderShell();
    });

    this.settingsBtnEl?.addEventListener("click", (ev) => {
      ev.stopPropagation();
      if (this.openNavMenu !== null) {
        this.openNavMenu = null;
        this.navOpenedByHover = false;
        this.syncNavMenus();
      }
      this.settingsOpen = !this.settingsOpen;
      this.syncSettingsPopover();
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-ui-theme]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const uiTheme = btn.dataset.uiTheme as UiTheme;
        if (uiTheme !== "dark" && uiTheme !== "light") return;
        this.updatePrefs({ uiTheme });
        this.syncSettingsPopover();
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-board-theme]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const boardTheme = btn.dataset.boardTheme as BoardTheme;
        this.updatePrefs({ boardTheme });
        this.syncSettingsPopover();
      });
    });

    this.root.querySelector("#btn-nums")?.addEventListener("click", () => {
      this.updatePrefs({ showNumbers: !this.showNumbers });
      this.syncSettingsPopover();
      this.syncBoard();
    });

    this.root.querySelector("#btn-diag")?.addEventListener("click", () => {
      this.updatePrefs({ showMainDiagonal: !this.showMainDiagonal });
      this.syncSettingsPopover();
      this.syncBoard();
    });

    this.root.querySelector("#btn-black-first")?.addEventListener("click", () => {
      this.setBlackFirst(!this.prefs.blackFirst);
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-first-mover]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const enabled = btn.dataset.firstMover === "dark";
        if (enabled === this.prefs.blackFirst) return;
        this.setBlackFirst(enabled);
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-underboard-tab]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const tab = btn.dataset.underboardTab as "opening" | "fen";
        if (tab !== "opening" && tab !== "fen") return;
        this.underboardTab = tab;
        if (this.mode === "analysis") {
          this.activeToolItem = tab === "fen" ? "import" : this.activeToolItem === "import" ? "opening" : this.activeToolItem;
          this.syncNavMenus();
        }
        this.syncUnderboardTabs();
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-side]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const side = btn.dataset.side as Color;
        if (side !== "light" && side !== "dark") return;
        this.humanColor = side;
        this.orientation = side;
        this.root.querySelectorAll<HTMLButtonElement>("[data-side]").forEach((b) => {
          b.classList.toggle("is-active", b.dataset.side === side);
        });
        void this.startNewGame();
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-level]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const lvl = Number(btn.dataset.level);
        if (!Number.isFinite(lvl)) return;
        this.aiLevel = lvl;
        this.root.querySelectorAll<HTMLButtonElement>("[data-level]").forEach((b) => {
          b.classList.toggle("is-active", Number(b.dataset.level) === lvl);
        });
      });
    });

    this.root.querySelectorAll<HTMLButtonElement>("[data-tc]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const tcId = btn.dataset.tc;
        if (!tcId) return;
        this.timeControlId = tcId;
        this.root.querySelectorAll<HTMLButtonElement>("[data-tc]").forEach((b) => {
          b.classList.toggle("is-active", b.dataset.tc === tcId);
        });
        void this.startNewGame();
      });
    });

    // Replay bar
    this.root.querySelector("#nav-start")?.addEventListener("click", () => {
      this.cursor = 0;
      void this.inspectCursor();
    });
    this.root.querySelector("#nav-prev")?.addEventListener("click", () => {
      if (this.cursor > 0) {
        this.cursor -= 1;
        void this.inspectCursor();
      }
    });
    this.root.querySelector("#nav-next")?.addEventListener("click", () => {
      if (this.cursor < this.timeline.length - 1) {
        this.cursor += 1;
        void this.inspectCursor();
      }
    });
    this.root.querySelector("#nav-end")?.addEventListener("click", () => {
      this.cursor = this.timeline.length - 1;
      void this.inspectCursor();
    });
    this.root.querySelector("#btn-flip")?.addEventListener("click", () => {
      this.orientation = this.orientation === "light" ? "dark" : "light";
      this.syncBoard();
      this.renderPlayerBars();
      this.renderEvalBar();
    });

    // Action bar
    this.root.querySelector("#btn-new")?.addEventListener("click", () => {
      if (this.mode === "basics") {
        void this.loadBasicsStep(this.activeBasicsLessonIdx, this.activeBasicsStepIdx);
      } else if (this.mode === "puzzles") {
        void this.loadPuzzle(this.activePuzzleIdx);
      } else if (this.mode === "practice") {
        void this.loadPracticeDrill(this.activeModuleIdx, this.activeDrillIdx);
      } else {
        void this.startNewGame();
      }
    });
    this.root.querySelector("#btn-leave-room")?.addEventListener("click", () => {
      this.leaveOnlineRoom(true);
    });
    this.root.querySelector("#btn-undo")?.addEventListener("click", () => void this.handleUndo());
    this.root.querySelector("#btn-resign")?.addEventListener("click", () => this.handleResignClick());

    // FEN / PDN I/O
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
    this.clearResignConfirm(false);
    try {
      const res = await this.engine.importPdn(pdn);
      if (switchToAnalysis) {
        this.leaveOnlineRoom(false);
        this.mode = "analysis";
        this.analysisEnabled = true;
        this.activeToolItem = "analysis";
        this.underboardTab = "opening";
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
    this.clearResignConfirm(false);
    this.stopClock();
    this.cancelAnalysis();
    this.forcedOutcome = null;
    this.aiThinking = false;
    this.lastAnalysis = null;
    this.lastAnalysisMs = 0;
    void this.engine.clearCache();

    const effectiveFen =
      customFen ??
      (this.prefs.blackFirst &&
      (this.mode === "analysis" || this.mode === "ai" || this.mode === "local")
        ? BLACK_FIRST_START_FEN
        : undefined);
    const snap = await this.engine.inspect(effectiveFen);
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
    if (this.mode === "basics") {
      return (
        this.isAtLatestPly() &&
        (this.basicsStatus === "trying" || this.basicsStatus === "wrong")
      );
    }
    if (this.mode === "puzzles") {
      return (
        this.isAtLatestPly() &&
        (this.puzzleStatus === "solving" || this.puzzleStatus === "wrong")
      );
    }
    if (this.mode === "practice") {
      return (
        this.isAtLatestPly() &&
        this.currentSnapshot.side_to_move === this.activePracticeDrill().humanColor
      );
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
    this.clearResignConfirm();
    if (this.mode === "online") {
      this.onlineConn?.sendMove(move.long_notation);
      return;
    }
    if (this.mode === "basics") {
      await this.handleBasicsUserMove(move);
      return;
    }
    if (this.mode === "puzzles") {
      await this.handlePuzzleUserMove(move);
      return;
    }
    this.practiceHintArrow = null;
    await this.commitNotation(move.long_notation);
  }

  private async commitNotation(moveNotation: string): Promise<void> {
    if (!this.currentSnapshot) return;
    this.clearResignConfirm(false);
    this.cancelAnalysis();
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
    this.lastAnalysisMs = 0;

    const tc = this.activeTimeControl();
    if (tc.initialMs > 0 && (this.mode === "ai" || this.mode === "local")) {
      this.clocksMs[mover] += tc.incrementMs;
    }
    const outcome = this.effectiveOutcome();
    if (outcome.type !== "ongoing") {
      this.stopClock();
      if (this.mode === "practice") {
        const drill = this.activePracticeDrill();
        if (outcome.type === "win" && outcome.winner === drill.humanColor) {
          this.markDrillCompleted(drill.id);
        }
      }
    }

    this.syncBoard();
    this.refreshPanels();
    void this.triggerAnalysisIfEnabled();
    void this.maybeTriggerAiTurn();
  }

  private async maybeTriggerAiTurn(): Promise<void> {
    if ((this.mode !== "ai" && this.mode !== "practice") || !this.currentSnapshot) return;
    if (this.effectiveOutcome().type !== "ongoing") return;
    if (!this.isAtLatestPly()) return;
    const targetHuman =
      this.mode === "practice" ? this.activePracticeDrill().humanColor : this.humanColor;
    const level = this.mode === "practice" ? this.activePracticeDrill().aiLevel : this.aiLevel;
    if (this.currentSnapshot.side_to_move === targetHuman) return;

    this.aiThinking = true;
    this.syncBoard();
    this.refreshPanels();

    const fen = this.currentSnapshot.fen;
    const hashes = this.historyHashesUpTo(this.cursor);
    const modeAtStart = this.mode;
    try {
      const { chosen, analysis } = await this.engine.aiMove(fen, level, hashes);
      this.aiThinking = false;
      if (this.currentSnapshot?.fen !== fen || this.mode !== modeAtStart) return;
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

  private cancelAnalysis(): void {
    this.analysisSeq += 1;
    this.analysisBusy = false;
    this.engine.cancelAnalysis();
  }

  private async triggerAnalysisIfEnabled(): Promise<void> {
    if (this.mode === "online" && this.onlineState?.status === "active") {
      this.cancelAnalysis();
      this.lastAnalysis = null;
      this.lastAnalysisMs = 0;
      this.renderAnalysisPanel();
      this.renderEvalBar();
      return;
    }
    if (!this.analysisEnabled || !this.currentSnapshot) {
      this.cancelAnalysis();
      return;
    }
    if (this.effectiveOutcome().type !== "ongoing") {
      this.cancelAnalysis();
      this.lastAnalysis = null;
      this.lastAnalysisMs = 0;
      this.board.setArrows(this.computeBoardArrows());
      this.renderAnalysisPanel();
      this.renderEvalBar();
      return;
    }

    const seq = ++this.analysisSeq;
    const fen = this.currentSnapshot.fen;
    const hashes = this.historyHashesUpTo(this.cursor);
    this.analysisBusy = true;
    this.lastAnalysis = null;
    this.lastAnalysisMs = 0;
    this.board.setArrows(this.computeBoardArrows());
    this.renderAnalysisPanel();
    this.renderEvalBar();

    const t0 = performance.now();
    try {
      const res = await this.engine.analyze(fen, {
        maxDepth: this.analysisDepth,
        maxNodes: this.analysisTimeMs === 0 ? 0 : undefined,
        maxTimeMs: this.analysisTimeMs > 0 ? this.analysisTimeMs : undefined,
        threads: this.analysisThreads,
        multipv: this.analysisMultiPv,
        historyHashes: hashes,
        legalMoves: this.currentSnapshot.legal_moves.map((m) => m.long_notation),
        onProgress: (update) => {
          if (seq !== this.analysisSeq) return;
          this.lastAnalysis = {
            depth: update.depth,
            nodes: update.nodes,
            lines: update.lines,
          };
          this.lastAnalysisMs = Math.max(1, Math.round(performance.now() - t0));
          this.board.setArrows(this.computeBoardArrows());
          this.renderAnalysisPanel();
          this.renderEvalBar();
        },
      });
      if (seq !== this.analysisSeq) return;
      this.lastAnalysis = res;
      this.lastAnalysisMs = Math.max(1, Math.round(performance.now() - t0));
      this.analysisBusy = false;
      this.board.setArrows(this.computeBoardArrows());
      this.renderAnalysisPanel();
      this.renderEvalBar();
    } catch (err) {
      if (seq !== this.analysisSeq || err instanceof AnalysisCancelledError) return;
      this.analysisBusy = false;
      this.renderAnalysisPanel();
    }
  }

  private async inspectCursor(): Promise<void> {
    const entry = this.timeline[this.cursor];
    if (!entry) return;
    this.cancelAnalysis();
    if (this.mode === "online" && this.onlineState && this.isAtLatestPly()) {
      this.currentSnapshot = this.snapshotFromOnline(this.onlineState);
      this.syncBoard();
      this.refreshPanels();
      return;
    }
    const hashes = this.historyHashesUpTo(this.cursor);
    this.currentSnapshot = await this.engine.inspect(entry.fen, hashes);
    this.lastAnalysis = null;
    this.lastAnalysisMs = 0;
    this.syncBoard();
    this.refreshPanels();
    void this.triggerAnalysisIfEnabled();
  }

  private canUndoNow(): boolean {
    return !(
      this.mode === "online" ||
      this.mode === "puzzles" ||
      this.mode === "basics" ||
      this.cursor === 0 ||
      this.aiThinking
    );
  }

  private canResignNow(): boolean {
    if (this.mode === "puzzles" || this.mode === "basics") return false;
    if (this.effectiveOutcome().type !== "ongoing") return false;
    if (this.mode === "online") {
      return Boolean(
        this.onlineRoomId &&
          this.onlineRole !== "spectator" &&
          this.onlineState?.status === "active",
      );
    }
    return Boolean(this.currentSnapshot);
  }

  private clearResignConfirm(syncUi = true): void {
    if (this.resignConfirmTimer !== null) {
      clearTimeout(this.resignConfirmTimer);
      this.resignConfirmTimer = null;
    }
    if (!this.resignConfirming) return;
    this.resignConfirming = false;
    if (syncUi) this.syncActionButtons();
  }

  private armResignConfirm(): void {
    if (!this.canResignNow()) return;
    if (this.resignConfirmTimer !== null) {
      clearTimeout(this.resignConfirmTimer);
    }
    this.resignConfirming = true;
    this.resignConfirmTimer = window.setTimeout(() => {
      this.resignConfirmTimer = null;
      if (this.resignConfirming) {
        this.resignConfirming = false;
        this.syncActionButtons();
      }
    }, 3500);
    this.syncActionButtons();
  }

  private syncActionButtons(): void {
    const t = this.t();
    const undoBtn = this.root.querySelector<HTMLButtonElement>("#btn-undo");
    if (undoBtn) {
      undoBtn.disabled = !this.canUndoNow();
    }
    const resignBtn = this.root.querySelector<HTMLButtonElement>("#btn-resign");
    if (resignBtn) {
      const canResign = this.canResignNow();
      if (!canResign && this.resignConfirming) {
        this.clearResignConfirm(false);
      }
      resignBtn.disabled = !canResign;
      resignBtn.classList.toggle("is-confirming", this.resignConfirming && canResign);
      const nextHtml =
        this.resignConfirming && canResign
          ? `${ICONS.check}<span>${escapeHtml(t.confirmResign)}</span>`
          : `${ICONS.resign}<span>${escapeHtml(t.resign)}</span>`;
      if (resignBtn.innerHTML !== nextHtml) {
        resignBtn.innerHTML = nextHtml;
      }
    }
  }

  private async handleUndo(): Promise<void> {
    if (!this.canUndoNow()) {
      return;
    }
    this.clearResignConfirm(false);
    const steps = (this.mode === "ai" || this.mode === "practice") && this.cursor >= 2 ? 2 : 1;
    this.timeline = this.timeline.slice(0, this.cursor - steps + 1);
    this.cursor = this.timeline.length - 1;
    this.forcedOutcome = null;
    await this.inspectCursor();
  }

  private handleResignClick(): void {
    if (!this.canResignNow()) {
      this.clearResignConfirm();
      return;
    }
    if (!this.resignConfirming) {
      this.armResignConfirm();
      return;
    }
    this.clearResignConfirm(false);
    this.performResign();
  }

  private performResign(): void {
    if (this.mode === "online") {
      if (this.onlineRoomId && this.onlineRole !== "spectator") {
        this.onlineConn?.resign();
      }
      this.syncActionButtons();
      return;
    }
    if (!this.currentSnapshot || this.effectiveOutcome().type !== "ongoing") {
      this.syncActionButtons();
      return;
    }
    const loser = this.mode === "ai" ? this.humanColor : this.currentSnapshot.side_to_move;
    this.forcedOutcome = {
      type: "win",
      winner: loser === "light" ? "dark" : "light",
      reason: "resignation",
    };
    this.stopClock();
    this.cancelAnalysis();
    this.syncBoard();
    this.refreshPanels();
  }

  private handleKeydown(ev: KeyboardEvent): void {
    if (ev.key === "Escape") {
      let handled = false;
      if (this.resignConfirming) {
        this.clearResignConfirm();
        handled = true;
      }
      if (this.openNavMenu !== null) {
        this.openNavMenu = null;
        this.navOpenedByHover = false;
        this.syncNavMenus();
        handled = true;
      }
      if (this.settingsOpen) {
        this.settingsOpen = false;
        this.syncSettingsPopover();
        handled = true;
      }
      if (handled) return;
    }
    if (
      ev.target instanceof HTMLTextAreaElement ||
      ev.target instanceof HTMLInputElement ||
      ev.target instanceof HTMLSelectElement
    ) {
      return;
    }
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
    this.renderUserChip();

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
      this.renderUserChip();
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
    this.renderUserChip();
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
    this.clearResignConfirm(false);
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
    if (this.mode === "basics") {
      return this.basicsHintArrow ? [this.basicsHintArrow] : [];
    }
    if (this.mode === "puzzles") {
      return this.puzzleHintArrow ? [this.puzzleHintArrow] : [];
    }
    const arrows: BoardArrow[] = [];
    if (this.mode === "practice" && this.practiceHintArrow) {
      arrows.push(this.practiceHintArrow);
    }
    if (this.analysisEnabled && this.lastAnalysis && this.lastAnalysis.lines.length > 0) {
      const palette = ["#15781B", "#003088", "#e68f00"];
      const opacities = [0.84, 0.56, 0.48];
      const widths = [14, 11, 9];
      this.lastAnalysis.lines.slice(0, 3).forEach((line, i) => {
        arrows.push({
          from: line.best_move.from,
          to: line.best_move.to,
          path: line.best_move.path,
          color: palette[i] ?? "#003088",
          opacity: opacities[i] ?? 0.48,
          width: widths[i] ?? 9,
        });
      });
    }
    return arrows;
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
    if (this.mode === "basics" || this.mode === "puzzles" || this.mode === "practice") {
      this.renderStudyPanel();
    }
    this.renderEvalBar();
    this.renderAnalysisPanel();
    this.renderOpeningPanel();
    this.renderMovesPanel();
    this.syncActionButtons();
  }

  private renderPlayerBars(): void {
    const t = this.t();
    const topColor: Color = this.orientation === "light" ? "dark" : "light";
    const bottomColor: Color = this.orientation;
    const tc = this.activeTimeControl();

    if (this.mode === "online" && this.onlineState) {
      const s = this.onlineState;
      const activeTurn = s.status === "active" && s.outcome.type === "ongoing" ? s.side_to_move : null;
      const timed = s.initial_ms > 0;
      const renderOnlineBar = (el: HTMLElement, c: Color) => {
        const seat = c === "light" ? s.light : s.dark;
        const name = seat
          ? escapeHtml(seat.name)
          : `${c === "light" ? t.lightShort : t.darkShort} (${t.waitingOpponent})`;
        const ratingBadge =
          seat && seat.rating !== null ? `<span class="li-badge">${Math.round(seat.rating)}</span>` : "";
        const offlineBadge =
          seat && !seat.connected ? `<span class="li-badge is-warn">${t.disconnected}</span>` : "";
        const running = timed && activeTurn === c;
        const low = running && this.clocksMs[c] < 15_000;
        const clockHtml = timed
          ? `<div class="player-clock ${running ? "is-running" : ""} ${low ? "is-low" : ""}">${formatClockMs(
              this.clocksMs[c],
            )}</div>`
          : "";
        el.innerHTML = `
          <div class="player-info">
            <div class="player-name">
              <span class="color-dot is-${c}"></span>
              <span class="player-name-text">${name}</span>
              ${ratingBadge}
              ${offlineBadge}
            </div>
          </div>
          ${clockHtml}
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
        return `${c === "light" ? t.lightShort : t.darkShort} — AI L${this.aiLevel}`;
      }
      return c === "light" ? t.lightSide : t.darkSide;
    };

    const timed = tc.initialMs > 0 && (this.mode === "ai" || this.mode === "local");
    const renderBar = (el: HTMLElement, c: Color) => {
      const running = timed && activeTurn === c;
      const low = running && this.clocksMs[c] < 15_000;
      const clockHtml = timed
        ? `<div class="player-clock ${running ? "is-running" : ""} ${low ? "is-low" : ""}">${formatClockMs(
            this.clocksMs[c],
          )}</div>`
        : "";
      el.innerHTML = `
        <div class="player-info">
          <div class="player-name">
            <span class="color-dot is-${c}"></span>
            <span class="player-name-text">${escapeHtml(labelFor(c))}</span>
          </div>
        </div>
        ${clockHtml}
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
    const captureBadge =
      snap && snap.has_capture && outcome.type === "ongoing"
        ? `<span class="li-badge is-warn">${t.mandatoryCapture}</span>`
        : "";
    const specBadge =
      this.mode === "online" && this.onlineState
        ? `<span class="li-badge is-accent">${ICONS.eye} ${
            this.onlineRole === "spectator" ? t.spectating : `${t.spectators}: ${this.onlineState.spectators}`
          }</span>`
        : "";

    let ratingBadge = "";
    if (this.mode === "online" && this.onlineState?.rating_changes) {
      const rc = this.onlineState.rating_changes;
      const dl = Math.round(rc.light_after - rc.light_before);
      const dd = Math.round(rc.dark_after - rc.dark_before);
      ratingBadge = `<span class="li-badge">L ${dl >= 0 ? "+" : ""}${dl} / D ${dd >= 0 ? "+" : ""}${dd}</span>`;
    }

    const ruleMeterHtml =
      kingFullMoves > 0
        ? `
          <div style="display:flex;flex-direction:column;gap:4px;">
            <div class="status-strip-row" style="font-size:0.75rem;color:var(--text-muted);">
              <span>${t.sixteenMoveCounter}</span>
              <span class="li-badge ${kingFullMoves >= 12 ? "is-warn" : ""}">${kingFullMoves}/16</span>
            </div>
            <div class="rule-meter">
              <div class="rule-meter-fill" style="width:${Math.min(100, Math.round((kingFullMoves / 16) * 100))}%"></div>
            </div>
          </div>
        `
        : "";

    const pieces = snap?.pieces ?? [];
    let lightTotal = 0;
    let lightKings = 0;
    let darkTotal = 0;
    let darkKings = 0;
    for (const p of pieces) {
      if (p.color === "light") {
        lightTotal += 1;
        if (p.kind === "king") lightKings += 1;
      } else {
        darkTotal += 1;
        if (p.kind === "king") darkKings += 1;
      }
    }

    const pieceCountHtml = snap
      ? `
        <div class="status-strip-row" style="font-size:0.76rem;color:var(--text-muted);padding-top:4px;border-top:1px solid var(--border-subtle);">
          <span>${t.piecesCount}</span>
          <span style="display:inline-flex;align-items:center;gap:10px;font-family:var(--font-mono);color:var(--text);">
            <span style="display:inline-flex;align-items:center;gap:4px;">
              <span class="color-dot is-light" style="width:9px;height:9px;"></span>
              ${lightTotal}${lightKings > 0 ? ` (${lightKings}K)` : ""}
            </span>
            <span style="display:inline-flex;align-items:center;gap:4px;">
              <span class="color-dot is-dark" style="width:9px;height:9px;"></span>
              ${darkTotal}${darkKings > 0 ? ` (${darkKings}K)` : ""}
            </span>
          </span>
        </div>
      `
      : "";

    this.statusEl.innerHTML = `
      <div class="status-strip ${outcome.type !== "ongoing" ? "is-over" : ""}">
        <div class="status-strip-row">
          <span>${escapeHtml(text)}</span>
          ${captureBadge || specBadge || ratingBadge ? `<div style="display:flex;gap:6px;flex-wrap:wrap;">${captureBadge}${specBadge}${ratingBadge}</div>` : ""}
        </div>
        ${ruleMeterHtml}
        ${pieceCountHtml}
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
            <div class="li-box">
              <div class="li-box-header">
                <h2 class="li-box-title">Room ${escapeHtml(this.onlineRoomId)}</h2>
                <button type="button" class="li-btn is-sm is-danger" id="btn-inline-leave">${ICONS.leave} <span>${t.leaveRoom}</span></button>
              </div>
              <div class="li-box-body" style="display:flex;flex-direction:column;gap:8px;">
                <div class="invite-bar">
                  <code>${escapeHtml(url)}</code>
                  <button type="button" class="li-btn is-sm is-primary" id="btn-copy-invite">${ICONS.copy} <span>${t.copyInvite}</span></button>
                </div>
                ${
                  this.onlineState?.status === "finished"
                    ? `<div><button type="button" class="li-btn is-sm is-primary" id="btn-analyze-finished">${t.analyzeGame}</button></div>`
                    : ""
                }
              </div>
            </div>
          `;
        })()
      : "";

    const accountHtml = this.authUser
      ? `
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${ICONS.user} <span>${t.onlineAccount}</span></h2>
            <button type="button" class="li-btn is-sm" id="btn-logout">${t.logout}</button>
          </div>
          <div class="li-box-body">
            <div class="inline-row" style="justify-content:space-between;">
              <strong>${escapeHtml(this.authUser.username)}</strong>
              <span class="li-badge is-accent">${t.ratingLabel}: ${Math.round(this.authUser.rating)} (${this.authUser.wins}W-${this.authUser.losses}L-${this.authUser.draws}D)</span>
            </div>
          </div>
        </div>
      `
      : `
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${ICONS.user} <span>${t.onlineAccount}</span></h2>
          </div>
          <div class="li-box-body online-stack">
            <label class="field">
              <span class="field-label">${t.guestName}</span>
              <input class="li-input" id="inp-guest-name" value="${escapeHtml(this.guestName)}" maxlength="24" />
            </label>
            <div class="inline-row">
              <input class="li-input" id="inp-auth-user" placeholder="${t.username}" maxlength="24" />
              <input class="li-input" id="inp-auth-pass" type="password" placeholder="${t.password}" />
            </div>
            <div class="inline-row">
              <button type="button" class="li-btn is-sm is-primary" id="btn-login">${t.login}</button>
              <button type="button" class="li-btn is-sm" id="btn-register">${t.register}</button>
            </div>
          </div>
        </div>
      `;

    const roomsHtml =
      this.openRooms.length === 0
        ? `<div class="li-box-body badge-muted">${t.noOpenRooms}</div>`
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
                        ? `<button type="button" class="li-btn is-sm is-primary" data-join-room="${escapeHtml(r.id)}">${t.joinRoom}</button>`
                        : ""
                    }
                    <button type="button" class="li-btn is-sm" data-watch-room="${escapeHtml(r.id)}">${t.watchRoom}</button>
                  </div>
                </div>
              `;
            })
            .join("");

    const lobbyContentHtml = `
      <div class="li-box-body online-stack">
        ${
          this.onlineRoomId
            ? ""
            : `
              <div>
                <div class="badge-muted" style="margin-bottom:5px;">${t.quickMatch}</div>
                <div class="inline-row">
                  <button type="button" class="li-btn is-sm is-green" data-matchmake="12+5">12+5</button>
                  <button type="button" class="li-btn is-sm is-green" data-matchmake="5+3">5+3</button>
                  <button type="button" class="li-btn is-sm is-green" data-matchmake="3+2">3+2</button>
                </div>
              </div>
              <div class="inline-row">
                <select class="li-select" id="sel-online-tc">
                  ${["12+5", "5+3", "3+2", "unlimited"]
                    .map(
                      (tc) =>
                        `<option value="${tc}" ${this.onlineTimeControl === tc ? "selected" : ""}>${
                          tc === "unlimited" ? t.unlimited : tc
                        }</option>`,
                    )
                    .join("")}
                </select>
                <button type="button" class="li-btn is-sm" id="btn-create-private">${t.createPrivate}</button>
              </div>
              <div class="inline-row">
                <input class="li-input" id="inp-room-id" placeholder="${t.roomIdPlaceholder}" />
                <button type="button" class="li-btn is-sm is-primary" id="btn-join-id">${t.joinRoom}</button>
                <button type="button" class="li-btn is-sm" id="btn-watch-id">${t.watchRoom}</button>
              </div>
            `
        }
        <div class="inline-row" style="justify-content:space-between;">
          <span class="badge-muted">${t.openRooms}</span>
          <button type="button" class="li-btn is-sm" id="btn-refresh-lobby">${ICONS.refresh} <span>${t.refresh}</span></button>
        </div>
      </div>
      <div class="room-list">${roomsHtml}</div>
    `;

    const lbHtml =
      this.leaderboardPlayers.length === 0
        ? `<div class="li-box-body badge-muted">${t.noGamesYet}</div>`
        : `<div class="compact-table">
            ${this.leaderboardPlayers
              .slice(0, 10)
              .map(
                (p, idx) => `
                  <div class="table-row">
                    <span>#${idx + 1} <strong>${escapeHtml(p.username)}</strong></span>
                    <span class="badge-muted">${Math.round(p.rating)} (${p.wins}-${p.losses}-${p.draws})</span>
                  </div>
                `,
              )
              .join("")}
          </div>`;

    const gamesHtml =
      this.recentGames.length === 0
        ? `<div class="li-box-body badge-muted">${t.noGamesYet}</div>`
        : `<div class="compact-table">
            ${this.recentGames
              .slice(0, 8)
              .map(
                (g) => `
                  <div class="table-row">
                    <div class="room-meta">
                      <span class="room-players">${escapeHtml(g.light_name)} vs ${escapeHtml(g.dark_name)} (${escapeHtml(g.result)})</span>
                      <span class="room-sub">${escapeHtml(g.time_control)} • ${g.moves_count} plies • ${escapeHtml(g.reason)}</span>
                    </div>
                    <button type="button" class="li-btn is-sm" data-analyze-game="${escapeHtml(g.id)}">${t.analyzeGame}</button>
                  </div>
                `,
              )
              .join("")}
          </div>`;

    const tabBody =
      this.onlineSubTab === "lobby"
        ? lobbyContentHtml
        : this.onlineSubTab === "leaderboard"
          ? lbHtml
          : gamesHtml;

    this.onlinePanelEl.innerHTML = `
      ${alertHtml}
      ${inviteHtml}
      ${accountHtml}
      <div class="li-box">
        <div class="li-tabs" role="tablist">
          <button type="button" role="tab" aria-selected="${this.onlineSubTab === "lobby"}" class="li-tab ${this.onlineSubTab === "lobby" ? "is-active" : ""}" data-online-tab="lobby">${t.tabLobby}</button>
          <button type="button" role="tab" aria-selected="${this.onlineSubTab === "leaderboard"}" class="li-tab ${this.onlineSubTab === "leaderboard" ? "is-active" : ""}" data-online-tab="leaderboard">${t.tabLeaderboard}</button>
          <button type="button" role="tab" aria-selected="${this.onlineSubTab === "recent"}" class="li-tab ${this.onlineSubTab === "recent" ? "is-active" : ""}" data-online-tab="recent">${t.tabRecentGames}</button>
        </div>
        ${tabBody}
      </div>
    `;

    this.bindOnlinePanelEvents();
  }

  private bindOnlinePanelEvents(): void {
    const panel = this.onlinePanelEl;
    if (!panel) return;

    panel.querySelectorAll<HTMLButtonElement>("[data-online-tab]").forEach((btn) => {
      btn.addEventListener("click", () => {
        const next = btn.dataset.onlineTab as "lobby" | "leaderboard" | "recent";
        if (next !== "lobby" && next !== "leaderboard" && next !== "recent") return;
        this.onlineSubTab = next;
        this.renderOnlinePanel();
      });
    });

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
      this.renderUserChip();
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
    const showEval = this.shouldShowEvalBar();
    this.boardStageEl.classList.toggle("no-eval", !showEval);

    const topLine = this.lastAnalysis?.lines[0];
    const snap = this.currentSnapshot;
    if (!showEval || !topLine || !snap) {
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
    if (
      (this.mode === "online" && this.onlineState?.status === "active") ||
      this.mode === "puzzles" ||
      this.mode === "basics" ||
      (this.mode === "practice" && !this.practiceLiveEval)
    ) {
      this.analysisPanelEl.style.display = "none";
      return;
    }
    this.analysisPanelEl.style.display = "";

    const t = this.t();
    const maxHw =
      typeof navigator !== "undefined" && navigator.hardwareConcurrency
        ? Math.max(1, Math.min(16, navigator.hardwareConcurrency))
        : 8;
    if (this.analysisThreads > maxHw) {
      this.analysisThreads = maxHw;
    }
    const timeIdx = timeMsToSliderIndex(this.analysisTimeMs);

    if (!this.analysisPanelEl.querySelector("#engine-controls-slot")) {
      this.analysisPanelEl.innerHTML = `
        <div class="engine-head">
          <div class="engine-head-left" id="engine-head-left"></div>
          <div class="engine-head-right">
            <button
              type="button"
              role="switch"
              aria-checked="${this.analysisEnabled}"
              class="li-switch ${this.analysisEnabled ? "is-on" : ""}"
              id="btn-toggle-engine"
              title="${escapeHtml(this.analysisEnabled ? t.engineOn : t.engineOff)}"
            ></button>
          </div>
        </div>
        <div class="engine-stats" id="engine-stats-slot"></div>
        <div class="engine-controls" id="engine-controls-slot">
          <div class="engine-slider-row">
            <label for="inp-threads">${escapeHtml(t.maxThreads)}</label>
            <input id="inp-threads" type="range" class="li-slider" min="1" max="${maxHw}" step="1" value="${this.analysisThreads}" />
            <span class="engine-slider-val" id="val-threads">${this.analysisThreads}</span>
          </div>
          <div class="engine-slider-row">
            <label for="inp-depth">${escapeHtml(t.maxDepthLabel)}</label>
            <input id="inp-depth" type="range" class="li-slider" min="1" max="24" step="1" value="${this.analysisDepth}" />
            <span class="engine-slider-val" id="val-depth">${this.analysisDepth}</span>
          </div>
          <div class="engine-slider-row">
            <label for="inp-time-ms">${escapeHtml(t.maxTimeMsLabel)}</label>
            <input id="inp-time-ms" type="range" class="li-slider" min="0" max="${ANALYSIS_TIME_STEPS_MS.length - 1}" step="1" value="${timeIdx}" />
            <span class="engine-slider-val" id="val-time-ms">${escapeHtml(formatAnalysisTime(this.analysisTimeMs))}</span>
          </div>
        </div>
        <div class="pv-list" id="pv-list-slot"></div>
      `;

      this.analysisPanelEl.querySelector("#btn-toggle-engine")?.addEventListener("click", () => {
        this.analysisEnabled = !this.analysisEnabled;
        if (!this.analysisEnabled) {
          this.cancelAnalysis();
        }
        this.board.setArrows(this.computeBoardArrows());
        this.renderEvalBar();
        this.renderAnalysisPanel();
        if (this.analysisEnabled) void this.triggerAnalysisIfEnabled();
      });

      const threadsInput = this.analysisPanelEl.querySelector<HTMLInputElement>("#inp-threads");
      const threadsVal = this.analysisPanelEl.querySelector<HTMLElement>("#val-threads");
      threadsInput?.addEventListener("input", () => {
        const val = Math.max(1, Math.min(maxHw, Math.round(Number(threadsInput.value) || 1)));
        this.analysisThreads = val;
        if (threadsVal) threadsVal.textContent = String(val);
      });
      threadsInput?.addEventListener("change", () => {
        const val = Math.max(1, Math.min(maxHw, Math.round(Number(threadsInput.value) || 1)));
        this.analysisThreads = val;
        if (threadsVal) threadsVal.textContent = String(val);
        if (this.analysisEnabled) void this.triggerAnalysisIfEnabled();
      });

      const depthInput = this.analysisPanelEl.querySelector<HTMLInputElement>("#inp-depth");
      const depthVal = this.analysisPanelEl.querySelector<HTMLElement>("#val-depth");
      depthInput?.addEventListener("input", () => {
        const val = Math.max(1, Math.min(24, Math.round(Number(depthInput.value) || 1)));
        this.analysisDepth = val;
        if (depthVal) depthVal.textContent = String(val);
      });
      depthInput?.addEventListener("change", () => {
        const val = Math.max(1, Math.min(24, Math.round(Number(depthInput.value) || 1)));
        this.analysisDepth = val;
        if (depthVal) depthVal.textContent = String(val);
        if (this.analysisEnabled) void this.triggerAnalysisIfEnabled();
      });

      const timeInput = this.analysisPanelEl.querySelector<HTMLInputElement>("#inp-time-ms");
      const timeVal = this.analysisPanelEl.querySelector<HTMLElement>("#val-time-ms");
      const readSliderTime = () => {
        const idx = Math.max(
          0,
          Math.min(ANALYSIS_TIME_STEPS_MS.length - 1, Math.round(Number(timeInput?.value) || 0)),
        );
        return ANALYSIS_TIME_STEPS_MS[idx] ?? 1500;
      };
      timeInput?.addEventListener("input", () => {
        this.analysisTimeMs = readSliderTime();
        if (timeVal) timeVal.textContent = formatAnalysisTime(this.analysisTimeMs);
      });
      timeInput?.addEventListener("change", () => {
        this.analysisTimeMs = readSliderTime();
        if (timeVal) timeVal.textContent = formatAnalysisTime(this.analysisTimeMs);
        if (this.analysisEnabled) void this.triggerAnalysisIfEnabled();
      });
    }

    const snap = this.currentSnapshot;
    const res = this.lastAnalysis;
    const topLine = res?.lines[0];
    const nps = res && this.lastAnalysisMs > 0 ? Math.round((res.nodes * 1000) / this.lastAnalysisMs) : 0;

    const evalText =
      this.analysisEnabled && topLine && snap
        ? formatEvalLight(topLine.score_light_cp, topLine.mate_in, snap.side_to_move)
        : this.analysisEnabled
          ? "…"
          : "—";

    const metaText = !this.analysisEnabled
      ? t.engineOff
      : this.analysisBusy
        ? res && res.depth > 0
          ? `d${res.depth}/${this.analysisDepth} • ${this.analysisThreads}T • ${t.engineThinking}`
          : t.engineThinking
        : res
          ? `d${res.depth}/${this.analysisDepth} • ${this.analysisThreads}T`
          : t.engineAnalysis;

    const headLeftEl = this.analysisPanelEl.querySelector<HTMLElement>("#engine-head-left");
    if (headLeftEl) {
      headLeftEl.innerHTML = `
        <span class="engine-eval-big">${escapeHtml(evalText)}</span>
        <span class="engine-meta">${escapeHtml(metaText)}</span>
      `;
    }

    const toggleBtn = this.analysisPanelEl.querySelector<HTMLButtonElement>("#btn-toggle-engine");
    if (toggleBtn) {
      toggleBtn.classList.toggle("is-on", this.analysisEnabled);
      toggleBtn.setAttribute("aria-checked", String(this.analysisEnabled));
      toggleBtn.title = this.analysisEnabled ? t.engineOn : t.engineOff;
    }

    const statsEl = this.analysisPanelEl.querySelector<HTMLElement>("#engine-stats-slot");
    const controlsEl = this.analysisPanelEl.querySelector<HTMLElement>("#engine-controls-slot");
    const pvListEl = this.analysisPanelEl.querySelector<HTMLElement>("#pv-list-slot");

    if (controlsEl) {
      controlsEl.hidden = !this.analysisEnabled;
    }

    if (statsEl) {
      statsEl.hidden = !this.analysisEnabled;
      if (this.analysisEnabled) {
        const nodesFull = res ? res.nodes.toLocaleString() : "0";
        const npsFull = res ? nps.toLocaleString() : "0";
        const nodesText = res ? formatCompactCount(res.nodes) : "0";
        const npsText = res ? formatCompactCount(nps) : "0";
        const depthText = res ? `${res.depth}/${this.analysisDepth}` : `0/${this.analysisDepth}`;
        const timeText = res && this.lastAnalysisMs > 0 ? formatElapsedMs(this.lastAnalysisMs) : "—";
        statsEl.innerHTML = `
          <span>${escapeHtml(t.depth)}: <strong>${depthText}</strong></span>
          <span title="${nodesFull} ${escapeHtml(t.nodesExplored)}"><strong>${nodesText}</strong> ${escapeHtml(t.nodesExplored)}</span>
          <span title="${npsFull} ${escapeHtml(t.nps)}"><strong>${npsText}</strong> ${escapeHtml(t.nps)}</span>
          <span><strong>${timeText}</strong></span>
        `;
      }
    }

    if (pvListEl) {
      pvListEl.hidden = !this.analysisEnabled;
      if (this.analysisEnabled) {
        if (res && snap && res.lines.length > 0) {
          const lines = res.lines.slice(0, 3);
          const rowsHtml = lines
            .map((line) => {
              const badge = formatEvalLight(line.score_light_cp, line.mate_in, snap.side_to_move);
              const scoreCls = line.score_light_cp >= 0 ? "is-light" : "is-dark";
              const bestToken = line.pv_notations[0] ?? "";
              return `
                <button type="button" class="pv-row" data-pv-move="${escapeHtml(bestToken)}">
                  <span class="pv-score ${scoreCls}">${badge}</span>
                  <span class="pv-line">${escapeHtml(line.pv_notations.join("  "))}</span>
                </button>
              `;
            })
            .join("");
          const emptyCount = Math.max(0, 3 - lines.length);
          const fillersHtml = `<div class="pv-row is-empty" aria-hidden="true"></div>`.repeat(emptyCount);
          pvListEl.innerHTML = `${rowsHtml}${fillersHtml}`;
        } else {
          pvListEl.innerHTML = `<div class="pv-placeholder">${escapeHtml(
            this.analysisBusy ? t.engineThinking : "—",
          )}</div>`;
        }

        pvListEl.querySelectorAll<HTMLButtonElement>("[data-pv-move]").forEach((btn) => {
          btn.addEventListener("click", () => {
            const tok = btn.dataset.pvMove;
            if (tok && this.canHumanMoveNow()) {
              void this.commitNotation(tok);
            }
          });
        });
      } else {
        pvListEl.innerHTML = "";
      }
    }
  }

  private async loadOpeningLine(lineId: string): Promise<void> {
    const line = OPENING_BOOK.find((l) => l.id === lineId);
    if (!line) return;
    this.clearResignConfirm(false);
    this.cancelAnalysis();
    void this.engine.clearCache();
    this.stopClock();
    this.forcedOutcome = null;
    this.lastAnalysis = null;
    this.lastAnalysisMs = 0;

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
        <div class="status-strip-row" style="margin-bottom:3px;">
          <span class="opening-formation-title" style="margin:0;">${escapeHtml(formationTitle ?? t.openingExplorer)}</span>
          <span class="li-badge">${match.matchingLines.length} ${t.openingLinesCount}</span>
        </div>
        <div class="opening-note">${escapeHtml(noteText)}</div>
      </div>
    `;

    const movesHtml =
      match.nextMoves.length > 0
        ? `<div>
            ${match.nextMoves
              .map((bm) => {
                const topLine = bm.lines[0];
                const label = topLine
                  ? isTh
                    ? `${topLine.nameTh} • ${topLine.variationTh}`
                    : `${topLine.nameEn} • ${topLine.variationEn}`
                  : "";
                return `
                  <button type="button" class="book-row" data-book-move="${escapeHtml(bm.notation)}">
                    <span class="book-not">${escapeHtml(bm.notation)}</span>
                    <span class="book-name">${escapeHtml(label)}</span>
                    <span class="book-count">${bm.lineCount} ${t.openingLinesCount}</span>
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
          return `<button type="button" class="formation-chip ${isActive ? "is-active" : ""}" data-formation-line="${escapeHtml(
            firstLine?.id ?? "",
          )}">${escapeHtml(isTh ? f.nameTh : f.nameEn)}</button>`;
        }).join("")}
      </div>
    `;

    this.openingPanelEl.innerHTML = `
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
    let tableInnerHtml = "";
    if (moves.length === 0) {
      const emptyMsg = this.timeline[0]?.fen.startsWith("B:") ? t.noMovesYetDark : t.noMovesYet;
      tableInnerHtml = `<div class="moves-empty">${escapeHtml(emptyMsg)}</div>`;
    } else {
      for (let i = 0; i < moves.length; i += 2) {
        const moveNum = Math.floor(i / 2) + 1;
        const lightPly = i + 1;
        const darkPly = i + 2;
        const lightTok = moves[i]?.notation ?? "";
        const darkTok = moves[i + 1]?.notation ?? "";
        tableInnerHtml += `
          <span class="move-num">${moveNum}.</span>
          <button type="button" class="move-cell ${this.cursor === lightPly ? "is-current" : ""}" data-ply="${lightPly}">${escapeHtml(lightTok)}</button>
          ${
            darkTok
              ? `<button type="button" class="move-cell ${this.cursor === darkPly ? "is-current" : ""}" data-ply="${darkPly}">${escapeHtml(darkTok)}</button>`
              : `<span class="move-cell"></span>`
          }
        `;
      }
    }

    this.movesPanelEl.innerHTML = `
      <div class="moves-wrap">
        <div class="moves-table">${tableInnerHtml}</div>
      </div>
    `;

    this.movesPanelEl.querySelectorAll<HTMLButtonElement>("[data-ply]").forEach((btn) => {
      btn.addEventListener("click", () => {
        this.cursor = Number(btn.dataset.ply);
        void this.inspectCursor();
      });
    });

    // Update replay bar disabled states
    const atStart = this.cursor === 0;
    const atEnd = this.cursor >= this.timeline.length - 1;
    const navStart = this.root.querySelector<HTMLButtonElement>("#nav-start");
    const navPrev = this.root.querySelector<HTMLButtonElement>("#nav-prev");
    const navNext = this.root.querySelector<HTMLButtonElement>("#nav-next");
    const navEnd = this.root.querySelector<HTMLButtonElement>("#nav-end");
    if (navStart) navStart.disabled = atStart;
    if (navPrev) navPrev.disabled = atStart;
    if (navNext) navNext.disabled = atEnd;
    if (navEnd) navEnd.disabled = atEnd;
    this.syncActionButtons();

    // Auto-scroll current move inside .moves-wrap only (never jumps outer page)
    const wrap = this.movesPanelEl.querySelector<HTMLElement>(".moves-wrap");
    const currentCell = wrap?.querySelector<HTMLElement>(".move-cell.is-current");
    if (wrap && currentCell) {
      const wrapRect = wrap.getBoundingClientRect();
      const cellRect = currentCell.getBoundingClientRect();
      if (cellRect.top < wrapRect.top) {
        wrap.scrollTop -= wrapRect.top - cellRect.top + 4;
      } else if (cellRect.bottom > wrapRect.bottom) {
        wrap.scrollTop += cellRect.bottom - wrapRect.bottom + 4;
      }
    }
  }

  // --- Puzzles & Practice (100% offline WASM) ---

  private saveStudyProgress(): void {
    try {
      localStorage.setItem(LS_STUDY_PROGRESS_KEY, JSON.stringify(this.studyProgress));
    } catch {
      // Ignore storage quota errors
    }
  }

  private activePuzzle(): KonMakhosPuzzle {
    return KON_MAKHOS_PUZZLES[this.activePuzzleIdx] ?? KON_MAKHOS_PUZZLES[0];
  }

  private activePracticeModule(): PracticeModule {
    return PRACTICE_MODULES[this.activeModuleIdx] ?? PRACTICE_MODULES[0];
  }

  private activePracticeDrill(): PracticeDrill {
    const mod = this.activePracticeModule();
    return mod.drills[this.activeDrillIdx] ?? mod.drills[0];
  }

  private async loadPuzzle(idx: number): Promise<void> {
    const safeIdx = ((idx % KON_MAKHOS_PUZZLES.length) + KON_MAKHOS_PUZZLES.length) % KON_MAKHOS_PUZZLES.length;
    this.activePuzzleIdx = safeIdx;
    this.puzzleStep = 0;
    this.puzzleStatus = "solving";
    this.puzzleShowHint = false;
    this.puzzleHintArrow = null;
    this.puzzleFailedAttempt = false;
    const seq = ++this.puzzleRunSeq;

    this.stopClock();
    this.clearResignConfirm(false);
    this.cancelAnalysis();
    this.forcedOutcome = null;
    this.aiThinking = false;
    this.lastAnalysis = null;
    this.lastAnalysisMs = 0;

    const puzzle = this.activePuzzle();
    const snap = await this.engine.inspect(puzzle.fen);
    if (seq !== this.puzzleRunSeq || this.mode !== "puzzles") return;

    this.orientation = snap.side_to_move;
    this.timeline = [{ fen: snap.fen, hash: snap.hash, notation: null, lastMove: null }];
    this.cursor = 0;
    this.currentSnapshot = snap;
    this.syncBoard();
    this.refreshPanels();
  }

  private async handlePuzzleUserMove(move: MoveDto): Promise<void> {
    if (!this.currentSnapshot) return;
    const puzzle = this.activePuzzle();
    const expected = puzzle.solution[this.puzzleStep];
    if (!expected) return;

    const isCorrect =
      move.long_notation === expected ||
      move.notation === expected ||
      move.short_notation === expected;

    if (!isCorrect) {
      this.puzzleStatus = "wrong";
      if (!this.puzzleFailedAttempt) {
        this.puzzleFailedAttempt = true;
        this.studyProgress.puzzleRating = Math.max(800, this.studyProgress.puzzleRating - 12);
        this.saveStudyProgress();
      }
      this.syncBoard();
      this.renderStudyPanel();
      return;
    }

    this.puzzleHintArrow = null;
    this.puzzleShowHint = false;
    this.puzzleStatus = "solving";
    const seq = ++this.puzzleRunSeq;
    await this.commitNotation(move.long_notation);
    if (seq !== this.puzzleRunSeq || this.mode !== "puzzles") return;

    this.puzzleStep += 1;
    if (this.puzzleStep >= puzzle.solution.length) {
      this.finishPuzzleSolved(puzzle);
      return;
    }

    this.aiThinking = true;
    this.syncBoard();
    this.renderStudyPanel();

    window.setTimeout(async () => {
      if (seq !== this.puzzleRunSeq || this.mode !== "puzzles") return;
      const reply = puzzle.solution[this.puzzleStep];
      this.aiThinking = false;
      if (!reply) return;
      await this.commitNotation(reply);
      if (seq !== this.puzzleRunSeq || this.mode !== "puzzles") return;
      this.puzzleStep += 1;
      if (this.puzzleStep >= puzzle.solution.length) {
        this.finishPuzzleSolved(puzzle);
      } else {
        this.syncBoard();
        this.renderStudyPanel();
      }
    }, 240);
  }

  private finishPuzzleSolved(puzzle: KonMakhosPuzzle): void {
    this.puzzleStatus = "solved";
    if (!this.puzzleFailedAttempt && !this.studyProgress.solvedPuzzles.includes(puzzle.id)) {
      const expectedScore = 1 / (1 + Math.pow(10, (puzzle.rating - this.studyProgress.puzzleRating) / 400));
      const gain = Math.max(8, Math.round(28 * (1 - expectedScore)));
      this.studyProgress.puzzleRating += gain;
      this.studyProgress.solvedPuzzles.push(puzzle.id);
      this.saveStudyProgress();
    } else if (!this.studyProgress.solvedPuzzles.includes(puzzle.id)) {
      this.studyProgress.solvedPuzzles.push(puzzle.id);
      this.saveStudyProgress();
    }
    this.syncBoard();
    this.renderStudyPanel();
  }

  private showPuzzleHint(): void {
    if (!this.currentSnapshot) return;
    const puzzle = this.activePuzzle();
    const expected = puzzle.solution[this.puzzleStep];
    if (!expected) return;
    const match = this.currentSnapshot.legal_moves.find(
      (m) => m.long_notation === expected || m.notation === expected || m.short_notation === expected,
    );
    if (match) {
      this.puzzleHintArrow = {
        from: match.from,
        to: match.to,
        path: match.path,
        color: "#e68f00",
        opacity: 0.9,
        width: 14,
      };
    }
    this.puzzleShowHint = true;
    this.syncBoard();
    this.renderStudyPanel();
  }

  private async revealPuzzleSolution(): Promise<void> {
    const puzzle = this.activePuzzle();
    this.puzzleFailedAttempt = true;
    this.puzzleHintArrow = null;
    this.puzzleShowHint = true;
    this.aiThinking = false;
    const seq = ++this.puzzleRunSeq;

    const startSnap = await this.engine.inspect(puzzle.fen);
    if (seq !== this.puzzleRunSeq || this.mode !== "puzzles") return;
    const timeline: HistoryEntry[] = [
      { fen: startSnap.fen, hash: startSnap.hash, notation: null, lastMove: null },
    ];
    let curFen = startSnap.fen;
    let curSnap = startSnap;
    const hashes = [startSnap.hash];
    for (const tok of puzzle.solution) {
      const res = await this.engine.applyMove(curFen, tok, hashes);
      if (seq !== this.puzzleRunSeq || this.mode !== "puzzles") return;
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
    this.puzzleStep = puzzle.solution.length;
    this.puzzleStatus = "revealed";
    this.syncBoard();
    this.refreshPanels();
  }

  private async loadPracticeDrill(moduleIdx: number, drillIdx: number): Promise<void> {
    this.clearResignConfirm(false);
    this.activeModuleIdx = Math.max(0, Math.min(PRACTICE_MODULES.length - 1, moduleIdx));
    const mod = this.activePracticeModule();
    this.activeDrillIdx = Math.max(0, Math.min(mod.drills.length - 1, drillIdx));
    const drill = this.activePracticeDrill();
    this.orientation = drill.humanColor;
    this.practiceHintArrow = null;
    this.analysisEnabled = this.practiceLiveEval;
    await this.startNewGame(drill.fen);
  }

  private markDrillCompleted(drillId: string): void {
    if (!this.studyProgress.completedDrills.includes(drillId)) {
      this.studyProgress.completedDrills.push(drillId);
      this.saveStudyProgress();
    }
  }

  private activeBasicsLesson(): BasicsLesson {
    return BASICS_LESSONS[this.activeBasicsLessonIdx] ?? BASICS_LESSONS[0];
  }

  private activeBasicsStep(): BasicsLessonStep {
    const lesson = this.activeBasicsLesson();
    return lesson.steps[this.activeBasicsStepIdx] ?? lesson.steps[0];
  }

  private markBasicsStepCompleted(stepId: string): void {
    if (!this.studyProgress.completedBasics.includes(stepId)) {
      this.studyProgress.completedBasics.push(stepId);
      this.saveStudyProgress();
    }
  }

  private async loadBasicsStep(lessonIdx: number, stepIdx: number): Promise<void> {
    const safeLessonIdx =
      ((lessonIdx % BASICS_LESSONS.length) + BASICS_LESSONS.length) % BASICS_LESSONS.length;
    const lesson = BASICS_LESSONS[safeLessonIdx];
    const safeStepIdx = Math.max(0, Math.min(lesson.steps.length - 1, stepIdx));

    this.activeBasicsLessonIdx = safeLessonIdx;
    this.activeBasicsStepIdx = safeStepIdx;
    this.basicsStatus = "trying";
    this.basicsShowHint = false;
    this.basicsHintArrow = null;
    const seq = ++this.basicsRunSeq;

    this.stopClock();
    this.clearResignConfirm(false);
    this.cancelAnalysis();
    this.forcedOutcome = null;
    this.aiThinking = false;
    this.lastAnalysis = null;
    this.lastAnalysisMs = 0;

    const step = lesson.steps[safeStepIdx];
    const snap = await this.engine.inspect(step.fen);
    if (seq !== this.basicsRunSeq || this.mode !== "basics") return;

    this.orientation = snap.side_to_move;
    this.timeline = [{ fen: snap.fen, hash: snap.hash, notation: null, lastMove: null }];
    this.cursor = 0;
    this.currentSnapshot = snap;
    this.syncBoard();
    this.refreshPanels();
  }

  private async handleBasicsUserMove(move: MoveDto): Promise<void> {
    if (!this.currentSnapshot) return;
    const step = this.activeBasicsStep();
    const isMatch = step.acceptedMoves.some(
      (tok) =>
        tok === move.long_notation ||
        tok === move.notation ||
        tok === move.short_notation,
    );

    if (!isMatch) {
      this.basicsStatus = "wrong";
      this.syncBoard();
      this.renderStudyPanel();
      return;
    }

    this.basicsHintArrow = null;
    this.basicsShowHint = false;
    const seq = ++this.basicsRunSeq;
    await this.commitNotation(move.long_notation);
    if (seq !== this.basicsRunSeq || this.mode !== "basics") return;

    this.markBasicsStepCompleted(step.id);
    this.basicsStatus = "completed";
    this.syncBoard();
    this.renderStudyPanel();
  }

  private showBasicsHint(): void {
    if (!this.currentSnapshot || this.mode !== "basics") return;
    const step = this.activeBasicsStep();
    if (step.demoArrow) {
      this.basicsHintArrow = {
        from: step.demoArrow.from,
        to: step.demoArrow.to,
        path: step.demoArrow.path,
        color: "#e68f00",
        opacity: 0.9,
        width: 14,
      };
    } else {
      const targetTok = step.acceptedMoves[0];
      const match = targetTok
        ? this.currentSnapshot.legal_moves.find(
            (m) =>
              m.long_notation === targetTok ||
              m.notation === targetTok ||
              m.short_notation === targetTok,
          )
        : undefined;
      if (match) {
        this.basicsHintArrow = {
          from: match.from,
          to: match.to,
          path: match.path,
          color: "#e68f00",
          opacity: 0.9,
          width: 14,
        };
      }
    }
    this.basicsShowHint = true;
    this.syncBoard();
    this.renderStudyPanel();
  }

  private advanceBasicsStep(): void {
    const lesson = this.activeBasicsLesson();
    if (this.activeBasicsStepIdx + 1 < lesson.steps.length) {
      void this.loadBasicsStep(this.activeBasicsLessonIdx, this.activeBasicsStepIdx + 1);
      return;
    }
    const nextLessonIdx = (this.activeBasicsLessonIdx + 1) % BASICS_LESSONS.length;
    void this.loadBasicsStep(nextLessonIdx, 0);
  }

  private async showPracticeHint(): Promise<void> {
    if (!this.currentSnapshot || this.mode !== "practice") return;
    const fen = this.currentSnapshot.fen;
    let top = this.lastAnalysis?.lines[0];
    if (!top) {
      try {
        const res = await this.engine.analyze(fen, {
          maxDepth: 6,
          maxNodes: 150_000,
          multipv: 1,
          historyHashes: this.historyHashesUpTo(this.cursor),
        });
        if (this.currentSnapshot?.fen !== fen || this.mode !== "practice") return;
        top = res.lines[0];
      } catch (err) {
        if (err instanceof AnalysisCancelledError) return;
        throw err;
      }
    }
    if (top) {
      this.practiceHintArrow = {
        from: top.best_move.from,
        to: top.best_move.to,
        path: top.best_move.path,
        color: "#15781B",
        opacity: 0.9,
        width: 14,
      };
      this.board.setArrows(this.computeBoardArrows());
    }
  }

  private renderStudyPanel(): void {
    if (!this.studyPanelEl) return;
    const t = this.t();
    const isTh = this.locale === "th";

    if (this.mode === "basics") {
      const lesson = this.activeBasicsLesson();
      const step = this.activeBasicsStep();
      const totalSteps = BASICS_LESSONS.reduce((acc, l) => acc + l.steps.length, 0);
      const completedCount = BASICS_LESSONS.reduce(
        (acc, l) =>
          acc + l.steps.filter((s) => this.studyProgress.completedBasics.includes(s.id)).length,
        0,
      );
      const stepDone = this.studyProgress.completedBasics.includes(step.id);
      const allDone = completedCount >= totalSteps;

      let feedbackHtml = `<div class="study-feedback">${escapeHtml(
        isTh ? step.promptTh : step.promptEn,
      )}</div>`;
      if (this.basicsStatus === "wrong") {
        feedbackHtml = `<div class="study-feedback is-bad">${escapeHtml(t.basicsWrong)}</div>`;
      } else if (this.basicsStatus === "completed") {
        feedbackHtml = `<div class="study-feedback is-good">${escapeHtml(
          allDone ? t.basicsAllDone : t.basicsCorrect,
        )}</div>`;
      } else if (this.basicsShowHint) {
        feedbackHtml = `<div class="study-feedback is-hint">${escapeHtml(
          isTh ? step.hintTh : step.hintEn,
        )}</div>`;
      }

      const curriculumHtml = BASICS_LESSONS.map((l, lIdx) => {
        const lDoneCount = l.steps.filter((s) =>
          this.studyProgress.completedBasics.includes(s.id),
        ).length;
        const lAllDone = lDoneCount === l.steps.length;
        const lActive = lIdx === this.activeBasicsLessonIdx;

        const stepsHtml = l.steps
          .map((s, sIdx) => {
            const sDone = this.studyProgress.completedBasics.includes(s.id);
            const sActive = lActive && sIdx === this.activeBasicsStepIdx;
            return `
              <button
                type="button"
                class="basics-step-btn ${sActive ? "is-active" : ""} ${sDone ? "is-done" : ""}"
                data-basics-lesson="${lIdx}"
                data-basics-step="${sIdx}"
              >
                <span>${sDone ? "✓ " : ""}${escapeHtml(isTh ? s.titleTh : s.titleEn)}</span>
                <span class="badge-muted">${sIdx + 1}/${l.steps.length}</span>
              </button>
            `;
          })
          .join("");

        return `
          <div class="basics-lesson-group">
            <button
              type="button"
              class="basics-lesson-head ${lActive ? "is-active" : ""} ${lAllDone ? "is-done" : ""}"
              data-basics-lesson="${lIdx}"
              data-basics-step="0"
            >
              <span>${lAllDone ? "✓ " : ""}${lIdx + 1}. ${escapeHtml(isTh ? l.titleTh : l.titleEn)}</span>
              <span class="badge-muted">${lDoneCount}/${l.steps.length}</span>
            </button>
            <div class="basics-steps-row">${stepsHtml}</div>
          </div>
        `;
      }).join("");

      this.studyPanelEl.innerHTML = `
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${ICONS.basics} <span>${escapeHtml(t.basicsTitle)}</span></h2>
            <span class="li-badge is-good">${escapeHtml(t.basicsCompleted)}: ${completedCount}/${totalSteps}</span>
          </div>
          <div class="li-box-body">
            <div class="basics-meta-row">
              <span class="li-badge is-accent">${escapeHtml(t.basicsLessonLabel)} ${this.activeBasicsLessonIdx + 1}/${BASICS_LESSONS.length} • ${escapeHtml(t.basicsStepLabel)} ${this.activeBasicsStepIdx + 1}/${lesson.steps.length}</span>
              <span class="badge-muted">${escapeHtml(isTh ? lesson.titleTh : lesson.titleEn)}</span>
            </div>
            <h3 class="basics-step-title">${stepDone ? "✓ " : ""}${escapeHtml(isTh ? step.titleTh : step.titleEn)}</h3>
            <p class="basics-explanation">${escapeHtml(isTh ? step.explanationTh : step.explanationEn)}</p>
            ${feedbackHtml}
            <div class="inline-row">
              <button type="button" class="li-btn is-sm" id="btn-basics-hint" ${this.basicsStatus === "completed" ? "disabled" : ""}>${ICONS.hint} <span>${escapeHtml(t.basicsHint)}</span></button>
              <button type="button" class="li-btn is-sm" id="btn-basics-retry">${ICONS.refresh} <span>${escapeHtml(t.basicsRetry)}</span></button>
              <button type="button" class="li-btn is-sm ${this.basicsStatus === "completed" ? "is-primary" : ""}" id="btn-basics-next"><span>${escapeHtml(t.basicsNext)}</span> ${ICONS.next}</button>
            </div>
          </div>
        </div>
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${escapeHtml(t.basicsLessonLabel)} 1–${BASICS_LESSONS.length}</h2>
            <span class="badge-muted">${completedCount}/${totalSteps}</span>
          </div>
          <div class="basics-curriculum">${curriculumHtml}</div>
        </div>
      `;

      this.studyPanelEl.querySelector("#btn-basics-hint")?.addEventListener("click", () => {
        this.showBasicsHint();
      });
      this.studyPanelEl.querySelector("#btn-basics-retry")?.addEventListener("click", () => {
        void this.loadBasicsStep(this.activeBasicsLessonIdx, this.activeBasicsStepIdx);
      });
      this.studyPanelEl.querySelector("#btn-basics-next")?.addEventListener("click", () => {
        this.advanceBasicsStep();
      });
      this.studyPanelEl
        .querySelectorAll<HTMLButtonElement>("[data-basics-lesson]")
        .forEach((btn) => {
          btn.addEventListener("click", () => {
            const lIdx = Number(btn.dataset.basicsLesson);
            const sIdx = Number(btn.dataset.basicsStep ?? 0);
            void this.loadBasicsStep(lIdx, sIdx);
          });
        });
      return;
    }

    if (this.mode === "puzzles") {
      const puzzle = this.activePuzzle();
      const isSolved = this.studyProgress.solvedPuzzles.includes(puzzle.id);

      let feedbackHtml = `<div class="study-feedback">${escapeHtml(t.puzzlePrompt)}</div>`;
      if (this.puzzleStatus === "wrong") {
        feedbackHtml = `<div class="study-feedback is-bad">${escapeHtml(t.puzzleWrong)}</div>`;
      } else if (this.puzzleStatus === "solved") {
        feedbackHtml = `<div class="study-feedback is-good">${escapeHtml(t.puzzleSolved)}</div>`;
      } else if (this.puzzleStatus === "revealed") {
        feedbackHtml = `<div class="study-feedback is-hint">${escapeHtml(t.puzzleRevealed)}</div>`;
      } else if (this.puzzleShowHint) {
        feedbackHtml = `<div class="study-feedback is-hint">${escapeHtml(
          isTh ? puzzle.hintTh : puzzle.hintEn,
        )}</div>`;
      }

      const listHtml = KON_MAKHOS_PUZZLES.map((p, idx) => {
        const done = this.studyProgress.solvedPuzzles.includes(p.id);
        const active = idx === this.activePuzzleIdx;
        return `
          <button type="button" class="study-item ${active ? "is-active" : ""} ${done ? "is-done" : ""}" data-puzzle-idx="${idx}">
            <span>${done ? "✓ " : ""}#${idx + 1} ${escapeHtml(isTh ? p.titleTh : p.titleEn)}</span>
            <span class="badge-muted">${p.rating}</span>
          </button>
        `;
      }).join("");

      this.studyPanelEl.innerHTML = `
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${t.puzzleTitle}</h2>
            <span class="li-badge is-accent">${t.puzzleRating}: ${this.studyProgress.puzzleRating}</span>
          </div>
          <div class="li-box-body">
            <div class="inline-row" style="justify-content:space-between;margin-bottom:8px;">
              <strong>${isSolved ? "✓ " : ""}${escapeHtml(isTh ? puzzle.titleTh : puzzle.titleEn)}</strong>
              <span class="li-badge">${escapeHtml(isTh ? puzzle.themeTh : puzzle.themeEn)} • ${puzzle.rating}</span>
            </div>
            ${feedbackHtml}
            <div class="inline-row">
              <button type="button" class="li-btn is-sm" id="btn-puzzle-hint">${ICONS.hint} <span>${t.puzzleHint}</span></button>
              <button type="button" class="li-btn is-sm" id="btn-puzzle-reveal">${ICONS.eye} <span>${t.puzzleShowSolution}</span></button>
              <button type="button" class="li-btn is-sm" id="btn-puzzle-retry">${t.puzzleRetry}</button>
              <button type="button" class="li-btn is-sm is-primary" id="btn-puzzle-next">${t.puzzleNext}</button>
            </div>
          </div>
        </div>
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${t.puzzleSolvedCount}: ${this.studyProgress.solvedPuzzles.length}/${KON_MAKHOS_PUZZLES.length}</h2>
          </div>
          <div class="study-list">${listHtml}</div>
        </div>
      `;

      this.studyPanelEl.querySelector("#btn-puzzle-hint")?.addEventListener("click", () => this.showPuzzleHint());
      this.studyPanelEl
        .querySelector("#btn-puzzle-reveal")
        ?.addEventListener("click", () => void this.revealPuzzleSolution());
      this.studyPanelEl
        .querySelector("#btn-puzzle-retry")
        ?.addEventListener("click", () => void this.loadPuzzle(this.activePuzzleIdx));
      this.studyPanelEl
        .querySelector("#btn-puzzle-next")
        ?.addEventListener("click", () => void this.loadPuzzle(this.activePuzzleIdx + 1));
      this.studyPanelEl.querySelectorAll<HTMLButtonElement>("[data-puzzle-idx]").forEach((btn) => {
        btn.addEventListener("click", () => {
          void this.loadPuzzle(Number(btn.dataset.puzzleIdx));
        });
      });
      return;
    }

    if (this.mode === "practice") {
      const mod = this.activePracticeModule();
      const drill = this.activePracticeDrill();
      const outcome = this.effectiveOutcome();

      let bannerHtml = `<div class="study-feedback">${escapeHtml(isTh ? drill.goalTh : drill.goalEn)}</div>`;
      if (outcome.type === "win" && outcome.winner === drill.humanColor) {
        bannerHtml = `<div class="study-feedback is-good">${escapeHtml(t.practiceWinMsg)}</div>`;
      } else if (outcome.type !== "ongoing") {
        bannerHtml = `<div class="study-feedback is-bad">${escapeHtml(t.practiceFailMsg)}</div>`;
      }

      const modulesHtml = PRACTICE_MODULES.map((m, idx) => {
        const active = idx === this.activeModuleIdx;
        return `<button type="button" class="module-tab ${active ? "is-active" : ""}" data-mod-idx="${idx}">${escapeHtml(
          isTh ? m.titleTh : m.titleEn,
        )}</button>`;
      }).join("");

      const drillsHtml = mod.drills
        .map((d, idx) => {
          const active = idx === this.activeDrillIdx;
          const done = this.studyProgress.completedDrills.includes(d.id);
          return `
            <button type="button" class="study-item ${active ? "is-active" : ""} ${done ? "is-done" : ""}" data-drill-idx="${idx}">
              <span>${done ? "✓ " : ""}${escapeHtml(isTh ? d.titleTh : d.titleEn)}</span>
              <span class="badge-muted">AI L${d.aiLevel}</span>
            </button>
          `;
        })
        .join("");

      this.studyPanelEl.innerHTML = `
        <div class="li-box">
          <div class="li-box-header">
            <h2 class="li-box-title">${t.practiceTitle}</h2>
            <span class="li-badge is-good">${t.practiceCompleted}: ${this.studyProgress.completedDrills.length}/${PRACTICE_MODULES.reduce((s, m) => s + m.drills.length, 0)}</span>
          </div>
          <div class="li-box-body">
            <div class="module-tabs">${modulesHtml}</div>
            <div class="opening-note" style="margin-bottom:8px;">${escapeHtml(
              isTh ? mod.descriptionTh : mod.descriptionEn,
            )}</div>
            ${bannerHtml}
            <div class="inline-row">
              <button type="button" class="li-btn is-sm is-primary" id="btn-practice-retry">${t.practiceRetry}</button>
              <button type="button" class="li-btn is-sm" id="btn-practice-hint">${ICONS.hint} <span>${t.practiceHint}</span></button>
              <button type="button" class="li-btn is-sm ${this.practiceLiveEval ? "is-active" : ""}" id="btn-practice-eval">Eval</button>
            </div>
          </div>
          <div class="study-list">${drillsHtml}</div>
        </div>
      `;

      this.studyPanelEl.querySelector("#btn-practice-retry")?.addEventListener("click", () => {
        void this.loadPracticeDrill(this.activeModuleIdx, this.activeDrillIdx);
      });
      this.studyPanelEl.querySelector("#btn-practice-hint")?.addEventListener("click", () => {
        void this.showPracticeHint();
      });
      this.studyPanelEl.querySelector("#btn-practice-eval")?.addEventListener("click", () => {
        this.practiceLiveEval = !this.practiceLiveEval;
        this.analysisEnabled = this.practiceLiveEval;
        if (!this.analysisEnabled) {
          this.cancelAnalysis();
        }
        this.board.setArrows(this.computeBoardArrows());
        this.renderEvalBar();
        this.renderAnalysisPanel();
        this.renderStudyPanel();
        if (this.analysisEnabled) void this.triggerAnalysisIfEnabled();
      });
      this.studyPanelEl.querySelectorAll<HTMLButtonElement>("[data-mod-idx]").forEach((btn) => {
        btn.addEventListener("click", () => {
          void this.loadPracticeDrill(Number(btn.dataset.modIdx), 0);
        });
      });
      this.studyPanelEl.querySelectorAll<HTMLButtonElement>("[data-drill-idx]").forEach((btn) => {
        btn.addEventListener("click", () => {
          void this.loadPracticeDrill(this.activeModuleIdx, Number(btn.dataset.drillIdx));
        });
      });
    }
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
