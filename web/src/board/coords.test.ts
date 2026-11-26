import { describe, expect, it } from "vitest";
import {
  MAIN_DIAGONAL_SQUARES,
  displayCellToSquare,
  rowColToSquare,
  squareToDisplayCell,
  squareToRowCol,
} from "./coords";

describe("TCA 2025 board coordinates", () => {
  it("maps Light bottom rank (1..4) and Dark top rank (29..32) per Article 4", () => {
    // Light perspective: bottom-left is 4, bottom-right dark is 1, top-right is 29
    expect(squareToDisplayCell(4, "light")).toEqual({ displayRow: 7, displayCol: 0 });
    expect(squareToDisplayCell(1, "light")).toEqual({ displayRow: 7, displayCol: 6 });
    expect(squareToDisplayCell(29, "light")).toEqual({ displayRow: 0, displayCol: 7 });
    expect(squareToDisplayCell(32, "light")).toEqual({ displayRow: 0, displayCol: 1 });

    // Dark perspective: bottom-left is 29, bottom-right dark is 32, top-right is 4
    expect(squareToDisplayCell(29, "dark")).toEqual({ displayRow: 7, displayCol: 0 });
    expect(squareToDisplayCell(32, "dark")).toEqual({ displayRow: 7, displayCol: 6 });
    expect(squareToDisplayCell(4, "dark")).toEqual({ displayRow: 0, displayCol: 7 });
    expect(squareToDisplayCell(1, "dark")).toEqual({ displayRow: 0, displayCol: 1 });
  });

  it("round-trips all 32 dark squares in both orientations", () => {
    for (let sq = 1; sq <= 32; sq++) {
      const { row, col } = squareToRowCol(sq);
      expect(rowColToSquare(row, col)).toBe(sq);

      for (const orientation of ["light", "dark"] as const) {
        const { displayRow, displayCol } = squareToDisplayCell(sq, orientation);
        expect(displayCellToSquare(displayRow, displayCol, orientation)).toBe(sq);
      }
    }
  });

  it("places main diagonal 4-29 bottom-left to top-right in both orientations", () => {
    for (const orientation of ["light", "dark"] as const) {
      for (const sq of MAIN_DIAGONAL_SQUARES) {
        const { displayRow, displayCol } = squareToDisplayCell(sq, orientation);
        expect(displayRow + displayCol).toBe(7);
      }
    }
  });
});
