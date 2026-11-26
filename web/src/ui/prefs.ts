import type { Locale } from "../i18n/messages";

export type UiTheme = "dark" | "light";
export type BoardTheme = "brown" | "blue" | "green" | "walnut";

export interface UiPrefs {
  uiTheme: UiTheme;
  boardTheme: BoardTheme;
  showNumbers: boolean;
  showMainDiagonal: boolean;
  blackFirst: boolean;
  locale: Locale;
}

export const LS_UI_PREFS_KEY = "limakhos_ui_prefs_v1";

export const BOARD_THEMES: ReadonlyArray<{
  id: BoardTheme;
  lightHex: string;
  darkHex: string;
}> = [
  { id: "brown", lightHex: "#f0d9b5", darkHex: "#b58863" },
  { id: "blue", lightHex: "#dee3e6", darkHex: "#8ca2ad" },
  { id: "green", lightHex: "#ffffdd", darkHex: "#86a666" },
  { id: "walnut", lightHex: "#d7b892", darkHex: "#7d5434" },
];

export const DEFAULT_UI_PREFS: UiPrefs = {
  uiTheme: "dark",
  boardTheme: "brown",
  showNumbers: true,
  showMainDiagonal: true,
  blackFirst: false,
  locale: "th",
};

function isUiTheme(value: unknown): value is UiTheme {
  return value === "dark" || value === "light";
}

function isBoardTheme(value: unknown): value is BoardTheme {
  return value === "brown" || value === "blue" || value === "green" || value === "walnut";
}

function isLocale(value: unknown): value is Locale {
  return value === "th" || value === "en";
}

function getStorage(): Storage | null {
  try {
    if (typeof localStorage === "undefined") {
      return null;
    }
    return localStorage;
  } catch {
    return null;
  }
}

export function loadUiPrefs(): UiPrefs {
  const storage = getStorage();
  if (!storage) {
    return { ...DEFAULT_UI_PREFS };
  }

  let raw: string | null = null;
  try {
    raw = storage.getItem(LS_UI_PREFS_KEY);
  } catch {
    return { ...DEFAULT_UI_PREFS };
  }

  if (!raw) {
    return { ...DEFAULT_UI_PREFS };
  }

  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { ...DEFAULT_UI_PREFS };
    }
    const obj = parsed as Record<string, unknown>;
    return {
      uiTheme: isUiTheme(obj.uiTheme) ? obj.uiTheme : DEFAULT_UI_PREFS.uiTheme,
      boardTheme: isBoardTheme(obj.boardTheme) ? obj.boardTheme : DEFAULT_UI_PREFS.boardTheme,
      showNumbers:
        typeof obj.showNumbers === "boolean" ? obj.showNumbers : DEFAULT_UI_PREFS.showNumbers,
      showMainDiagonal:
        typeof obj.showMainDiagonal === "boolean"
          ? obj.showMainDiagonal
          : DEFAULT_UI_PREFS.showMainDiagonal,
      blackFirst:
        typeof obj.blackFirst === "boolean" ? obj.blackFirst : DEFAULT_UI_PREFS.blackFirst,
      locale: isLocale(obj.locale) ? obj.locale : DEFAULT_UI_PREFS.locale,
    };
  } catch {
    return { ...DEFAULT_UI_PREFS };
  }
}

export function saveUiPrefs(prefs: UiPrefs): void {
  const storage = getStorage();
  if (!storage) {
    return;
  }

  const normalized: UiPrefs = {
    uiTheme: isUiTheme(prefs.uiTheme) ? prefs.uiTheme : DEFAULT_UI_PREFS.uiTheme,
    boardTheme: isBoardTheme(prefs.boardTheme) ? prefs.boardTheme : DEFAULT_UI_PREFS.boardTheme,
    showNumbers:
      typeof prefs.showNumbers === "boolean" ? prefs.showNumbers : DEFAULT_UI_PREFS.showNumbers,
    showMainDiagonal:
      typeof prefs.showMainDiagonal === "boolean"
        ? prefs.showMainDiagonal
        : DEFAULT_UI_PREFS.showMainDiagonal,
    blackFirst:
      typeof prefs.blackFirst === "boolean" ? prefs.blackFirst : DEFAULT_UI_PREFS.blackFirst,
    locale: isLocale(prefs.locale) ? prefs.locale : DEFAULT_UI_PREFS.locale,
  };

  try {
    storage.setItem(LS_UI_PREFS_KEY, JSON.stringify(normalized));
  } catch {
    // Ignore storage quota or security errors.
  }
}

export function applyThemeAttributes(prefs: Pick<UiPrefs, "uiTheme" | "boardTheme">): void {
  if (typeof document === "undefined" || !document.documentElement) {
    return;
  }
  const root = document.documentElement;
  root.dataset.theme = isUiTheme(prefs.uiTheme) ? prefs.uiTheme : DEFAULT_UI_PREFS.uiTheme;
  root.dataset.boardTheme = isBoardTheme(prefs.boardTheme)
    ? prefs.boardTheme
    : DEFAULT_UI_PREFS.boardTheme;
}
