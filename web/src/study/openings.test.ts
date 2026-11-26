import { describe, expect, it } from "vitest";
import { FORMATIONS, OPENING_BOOK, matchOpeningHistory } from "./openings";
import { callWasm } from "./wasmTestHelper";

describe("Canonical Thai Opening Book (เปิดหมาก)", () => {
  it("covers all 6 canonical formations for both Light and Dark with >= 18 lines", () => {
    expect(OPENING_BOOK.length).toBeGreaterThanOrEqual(24);
    for (const f of FORMATIONS) {
      const lightLines = OPENING_BOOK.filter(
        (l) => l.formationId === f.id && l.side === "light",
      );
      const darkLines = OPENING_BOOK.filter(
        (l) => l.formationId === f.id && l.side === "dark",
      );
      expect(lightLines.length, `Light lines for ${f.id}`).toBeGreaterThanOrEqual(2);
      expect(darkLines.length, `Dark lines for ${f.id}`).toBeGreaterThanOrEqual(2);
    }
  });

  it("verifies every single move in every opening line is 100% legal under TCA 2025 WASM rules", async () => {
    const start = await callWasm({ cmd: "inspect" });
    expect(start.ok).toBe("snapshot");
    if (start.ok !== "snapshot") return;

    for (const line of OPENING_BOOK) {
      let currentFen = start.fen;
      const hashes = [start.hash];

      for (let i = 0; i < line.moves.length; i++) {
        const moveToken = line.moves[i];
        const res = await callWasm({
          cmd: "apply_move",
          fen: currentFen,
          move: moveToken,
          history_hashes: hashes,
        });
        expect(
          res.ok,
          `Line ${line.id} failed at ply ${i + 1} (${moveToken}): ${
            res.ok === "error" ? res.message : ""
          }`,
        ).toBe("move_applied");

        if (res.ok === "move_applied") {
          currentFen = res.snapshot.fen;
          hashes.push(res.snapshot.hash);
          expect(res.snapshot.outcome.type).toBe("ongoing");
        }
      }
    }
  });

  it("matches prefixes and suggests next book moves accurately", () => {
    const rootMatch = matchOpeningHistory([]);
    expect(rootMatch.matchingLines.length).toBe(OPENING_BOOK.length);
    expect(rootMatch.nextMoves.length).toBeGreaterThanOrEqual(4);

    const stdMatch = matchOpeningHistory(["7-11", "26-22", "3-7"]);
    expect(stdMatch.matchedFormation?.id).toBe("standard");
    expect(stdMatch.nextMoves.some((m) => m.notation === "30-26")).toBe(true);
  });
});
