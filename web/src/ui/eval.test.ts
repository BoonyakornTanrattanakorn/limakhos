import { describe, expect, it } from "vitest";
import { evalToLightPercent, formatClockMs, formatEvalLight } from "./eval";

describe("evaluation and clock formatting", () => {
  it("maps centipawn scores symmetrically around 50%", () => {
    expect(evalToLightPercent(0)).toBe(50);
    expect(evalToLightPercent(400)).toBeGreaterThan(75);
    expect(evalToLightPercent(-400)).toBeLessThan(25);
    expect(evalToLightPercent(29_500)).toBe(98);
    expect(evalToLightPercent(-29_500)).toBe(2);
  });

  it("formats centipawns and mate scores from Light's perspective", () => {
    expect(formatEvalLight(145, null, "light")).toBe("+1.45");
    expect(formatEvalLight(-80, null, "dark")).toBe("-0.80");
    expect(formatEvalLight(29_990, 2, "light")).toBe("#2");
    expect(formatEvalLight(-29_990, 2, "dark")).toBe("-#2");
  });

  it("formats TCA clocks cleanly", () => {
    expect(formatClockMs(12 * 60_000)).toBe("12:00");
    expect(formatClockMs(65_000)).toBe("01:05");
    expect(formatClockMs(9_400)).toBe("9.4");
    expect(formatClockMs(0)).toBe("0.0");
  });
});
