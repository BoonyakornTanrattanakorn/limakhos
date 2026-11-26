export const MATE_THRESHOLD = 28_000;

/**
 * Convert a Light-perspective centipawn evaluation into a smooth 0..100 fill percentage
 * for the vertical evaluation bar (50 = equal, 100 = Light winning, 0 = Dark winning).
 */
export function evalToLightPercent(scoreLightCp: number): number {
  if (scoreLightCp >= MATE_THRESHOLD) return 98;
  if (scoreLightCp <= -MATE_THRESHOLD) return 2;
  const logistic = 1 / (1 + Math.exp(-scoreLightCp / 320));
  return Math.max(3, Math.min(97, Math.round(logistic * 1000) / 10));
}

/**
 * Format Light-perspective evaluation for badges (`+1.45`, `-0.30`, `#3`, `-#2`).
 */
export function formatEvalLight(scoreLightCp: number, mateInSide: number | null, sideToMove: "light" | "dark"): string {
  if (mateInSide !== null) {
    const lightMate = sideToMove === "light" ? mateInSide : -mateInSide;
    return lightMate > 0 ? `#${lightMate}` : `-#${Math.abs(lightMate)}`;
  }
  const pawns = scoreLightCp / 100;
  const sign = pawns > 0.005 ? "+" : "";
  return `${sign}${pawns.toFixed(2)}`;
}

/**
 * Format milliseconds into `MM:SS` (or `SS.d` under 10 seconds).
 */
export function formatClockMs(ms: number): string {
  const clamped = Math.max(0, ms);
  if (clamped < 10_000) {
    return (clamped / 1000).toFixed(1);
  }
  const totalSec = Math.ceil(clamped / 1000);
  const mins = Math.floor(totalSec / 60);
  const secs = totalSec % 60;
  return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}
