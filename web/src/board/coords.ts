import type { Color } from "../engine/types";

export const MAIN_DIAGONAL_SQUARES = new Set([4, 8, 11, 15, 18, 22, 25, 29]);

/**
 * Convert a TCA 2025 square number (1..=32) to logical (row, col),
 * where row 0 is Light's back rank (1..=4) and col 0 is Light's left edge.
 */
export function squareToRowCol(sq: number): { row: number; col: number } {
  if (sq < 1 || sq > 32) {
    throw new RangeError(`Square out of range (1..=32): ${sq}`);
  }
  const idx = sq - 1;
  const row = Math.floor(idx / 4);
  const posInRow = idx % 4;
  const col = 6 + (row & 1) - 2 * posInRow;
  return { row, col };
}

/**
 * Convert logical (row, col) to TCA 2025 square number (1..=32), or null for unused light squares.
 */
export function rowColToSquare(row: number, col: number): number | null {
  if (row < 0 || row > 7 || col < 0 || col > 7) return null;
  if (((row + col) & 1) !== 0) return null;
  const posInRow = (6 + (row & 1) - col) / 2;
  return row * 4 + posInRow + 1;
}

/**
 * Convert a TCA square number to visual grid coordinates (displayRow 0=top..7=bottom, displayCol 0=left..7=right).
 */
export function squareToDisplayCell(
  sq: number,
  orientation: Color,
): { displayRow: number; displayCol: number } {
  const { row, col } = squareToRowCol(sq);
  if (orientation === "light") {
    return { displayRow: 7 - row, displayCol: col };
  }
  return { displayRow: row, displayCol: 7 - col };
}

/**
 * Convert visual grid coordinates back to a TCA square number (or null on light squares).
 */
export function displayCellToSquare(
  displayRow: number,
  displayCol: number,
  orientation: Color,
): number | null {
  const row = orientation === "light" ? 7 - displayRow : displayRow;
  const col = orientation === "light" ? displayCol : 7 - displayCol;
  return rowColToSquare(row, col);
}

/**
 * Center coordinates of a square in an 800x800 SVG overlay coordinate space.
 */
export function squareToSvgCenter(sq: number, orientation: Color): { x: number; y: number } {
  const { displayRow, displayCol } = squareToDisplayCell(sq, orientation);
  return {
    x: displayCol * 100 + 50,
    y: displayRow * 100 + 50,
  };
}
