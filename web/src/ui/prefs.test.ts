import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  BOARD_THEMES,
  DEFAULT_UI_PREFS,
  LS_UI_PREFS_KEY,
  applyThemeAttributes,
  loadUiPrefs,
  saveUiPrefs,
  type UiPrefs,
} from "./prefs";

function createMemoryStorage(): Storage {
  const store = new Map<string, string>();
  return {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(key: string) {
      return store.get(key) ?? null;
    },
    key(index: number) {
      return Array.from(store.keys())[index] ?? null;
    },
    removeItem(key: string) {
      store.delete(key);
    },
    setItem(key: string, value: string) {
      store.set(key, String(value));
    },
  };
}

describe("ui prefs", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", createMemoryStorage());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("exposes expected defaults and Lichess board theme palettes", () => {
    expect(DEFAULT_UI_PREFS).toEqual({
      uiTheme: "dark",
      boardTheme: "brown",
      showNumbers: true,
      showMainDiagonal: true,
      blackFirst: false,
      locale: "th",
    });

    expect(BOARD_THEMES).toEqual([
      { id: "brown", lightHex: "#f0d9b5", darkHex: "#b58863" },
      { id: "blue", lightHex: "#dee3e6", darkHex: "#8ca2ad" },
      { id: "green", lightHex: "#ffffdd", darkHex: "#86a666" },
      { id: "walnut", lightHex: "#d7b892", darkHex: "#7d5434" },
    ]);
  });

  it("returns defaults when localStorage is empty or unavailable", () => {
    expect(loadUiPrefs()).toEqual(DEFAULT_UI_PREFS);

    vi.stubGlobal("localStorage", undefined);
    expect(loadUiPrefs()).toEqual(DEFAULT_UI_PREFS);
    expect(() => saveUiPrefs(DEFAULT_UI_PREFS)).not.toThrow();
  });

  it("falls back gracefully on invalid JSON or non-object values", () => {
    localStorage.setItem(LS_UI_PREFS_KEY, "{not-valid-json");
    expect(loadUiPrefs()).toEqual(DEFAULT_UI_PREFS);

    for (const badValue of ["null", "[]", '"dark"', "42", "true"]) {
      localStorage.setItem(LS_UI_PREFS_KEY, badValue);
      expect(loadUiPrefs()).toEqual(DEFAULT_UI_PREFS);
    }
  });

  it("validates individual fields and replaces only invalid ones with defaults", () => {
    localStorage.setItem(
      LS_UI_PREFS_KEY,
      JSON.stringify({
        uiTheme: "sepia",
        boardTheme: "walnut",
        showNumbers: false,
        showMainDiagonal: "yes",
        locale: "fr",
      }),
    );

    expect(loadUiPrefs()).toEqual({
      uiTheme: "dark",
      boardTheme: "walnut",
      showNumbers: false,
      showMainDiagonal: true,
      blackFirst: false,
      locale: "th",
    });
  });

  it("persists and loads valid preferences roundtrip", () => {
    const custom: UiPrefs = {
      uiTheme: "light",
      boardTheme: "green",
      showNumbers: false,
      showMainDiagonal: false,
      blackFirst: true,
      locale: "en",
    };

    saveUiPrefs(custom);
    expect(loadUiPrefs()).toEqual(custom);
  });

  it("handles localStorage errors without throwing", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("quota");
      },
    });

    expect(loadUiPrefs()).toEqual(DEFAULT_UI_PREFS);
    expect(() => saveUiPrefs(DEFAULT_UI_PREFS)).not.toThrow();
  });

  it("applies dataset theme attributes safely", () => {
    const dataset: Record<string, string> = {};
    vi.stubGlobal("document", { documentElement: { dataset } });

    applyThemeAttributes({ uiTheme: "light", boardTheme: "blue" });
    expect(dataset.theme).toBe("light");
    expect(dataset.boardTheme).toBe("blue");

    vi.stubGlobal("document", undefined);
    expect(() =>
      applyThemeAttributes({ uiTheme: "dark", boardTheme: "brown" }),
    ).not.toThrow();
  });
});
