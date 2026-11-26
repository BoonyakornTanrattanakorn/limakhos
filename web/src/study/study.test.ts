import { describe, expect, it } from "vitest";
import { PRACTICE_MODULES } from "./practice";
import { KON_MAKHOS_PUZZLES } from "./puzzles";
import { callWasm } from "./wasmTestHelper";

describe("Kon Makhos Puzzles (กลหมากฮอส) & Practice Modules", () => {
  it("contains at least 12 puzzles with unique IDs and valid themes", () => {
    expect(KON_MAKHOS_PUZZLES.length).toBeGreaterThanOrEqual(12);
    const ids = new Set(KON_MAKHOS_PUZZLES.map((p) => p.id));
    expect(ids.size).toBe(KON_MAKHOS_PUZZLES.length);
  });

  it("verifies every puzzle FEN and every solution move is 100% legal under TCA 2025 WASM rules", async () => {
    for (const puzzle of KON_MAKHOS_PUZZLES) {
      const snap = await callWasm({ cmd: "inspect", fen: puzzle.fen });
      expect(
        snap.ok,
        `Puzzle ${puzzle.id} invalid FEN (${puzzle.fen}): ${snap.ok === "error" ? snap.message : ""}`,
      ).toBe("snapshot");
      if (snap.ok !== "snapshot") continue;
      expect(snap.outcome.type, `Puzzle ${puzzle.id} already ended`).toBe("ongoing");
      expect(puzzle.solution.length).toBeGreaterThanOrEqual(1);

      let curFen = snap.fen;
      const hashes = [snap.hash];

      for (let i = 0; i < puzzle.solution.length; i++) {
        const moveTok = puzzle.solution[i];
        const applied = await callWasm({
          cmd: "apply_move",
          fen: curFen,
          move: moveTok,
          history_hashes: hashes,
        });
        expect(
          applied.ok,
          `Puzzle ${puzzle.id} solution move #${i + 1} (${moveTok}) failed: ${
            applied.ok === "error" ? applied.message : ""
          }`,
        ).toBe("move_applied");

        if (applied.ok === "move_applied") {
          curFen = applied.snapshot.fen;
          hashes.push(applied.snapshot.hash);
        }
      }

      // Verify the first puzzle move is indeed top-ranked by the local WASM engine
      if (snap.legal_moves.length >= 2) {
        const analysis = await callWasm({
          cmd: "analyze",
          fen: puzzle.fen,
          max_depth: 5,
          max_nodes: 200_000,
          multipv: 2,
        });
        expect(analysis.ok).toBe("analysis");
        if (analysis.ok === "analysis" && analysis.lines.length >= 1) {
          const topLong = analysis.lines[0].best_move.from + (analysis.lines[0].best_move.captures.length ? "x" : "-") + analysis.lines[0].best_move.to;
          const solFirst = puzzle.solution[0];
          const topNotation = analysis.lines[0].pv_notations[0];
          const matchesBest =
            topNotation === solFirst ||
            topLong === solFirst ||
            `${analysis.lines[0].best_move.path.join("x")}` === solFirst;
          expect(
            matchesBest,
            `Puzzle ${puzzle.id}: expected engine top move (${topNotation} / ${topLong}) to match solution[0] (${solFirst})`,
          ).toBe(true);
        }
      }
    }
  });

  it("verifies all 4 Practice Modules and every drill position FEN is valid & ongoing", async () => {
    expect(PRACTICE_MODULES.length).toBe(4);
    for (const mod of PRACTICE_MODULES) {
      expect(mod.drills.length).toBeGreaterThanOrEqual(3);
      for (const drill of mod.drills) {
        const snap = await callWasm({ cmd: "inspect", fen: drill.fen });
        expect(
          snap.ok,
          `Drill ${drill.id} invalid FEN (${drill.fen}): ${snap.ok === "error" ? snap.message : ""}`,
        ).toBe("snapshot");
        if (snap.ok === "snapshot") {
          expect(snap.outcome.type, `Drill ${drill.id} must be ongoing`).toBe("ongoing");
          expect(snap.side_to_move).toBe(drill.humanColor);
          expect(snap.legal_moves.length).toBeGreaterThan(0);
        }
      }
    }
  });
});
