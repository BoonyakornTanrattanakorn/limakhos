import { describe, expect, it } from "vitest";
import { BASICS_LESSONS } from "./basics";
import { callWasm } from "./wasmTestHelper";

describe("Learn Makhos Basics Curriculum (TCA 2025)", () => {
  it("defines 6 lessons with unique IDs, unique step IDs, and non-empty bilingual text", () => {
    expect(BASICS_LESSONS.length).toBe(6);

    const lessonIds = new Set<string>();
    const stepIds = new Set<string>();

    for (const lesson of BASICS_LESSONS) {
      expect(lesson.id.trim().length).toBeGreaterThan(0);
      expect(lessonIds.has(lesson.id)).toBe(false);
      lessonIds.add(lesson.id);

      expect(lesson.titleTh.trim().length).toBeGreaterThan(0);
      expect(lesson.titleEn.trim().length).toBeGreaterThan(0);
      expect(lesson.summaryTh.trim().length).toBeGreaterThan(0);
      expect(lesson.summaryEn.trim().length).toBeGreaterThan(0);
      expect(lesson.steps.length).toBeGreaterThanOrEqual(1);

      for (const step of lesson.steps) {
        expect(step.id.trim().length).toBeGreaterThan(0);
        expect(stepIds.has(step.id)).toBe(false);
        stepIds.add(step.id);

        expect(step.titleTh.trim().length).toBeGreaterThan(0);
        expect(step.titleEn.trim().length).toBeGreaterThan(0);
        expect(step.explanationTh.trim().length).toBeGreaterThan(0);
        expect(step.explanationEn.trim().length).toBeGreaterThan(0);
        expect(step.promptTh.trim().length).toBeGreaterThan(0);
        expect(step.promptEn.trim().length).toBeGreaterThan(0);
        expect(step.hintTh.trim().length).toBeGreaterThan(0);
        expect(step.hintEn.trim().length).toBeGreaterThan(0);
        expect(step.acceptedMoves.length).toBeGreaterThanOrEqual(1);
      }
    }
  });

  it("verifies every lesson step FEN is ongoing and all acceptedMoves are legal in TCA 2025 WASM", async () => {
    for (const lesson of BASICS_LESSONS) {
      for (const step of lesson.steps) {
        const snap = await callWasm({ cmd: "inspect", fen: step.fen });
        expect(
          snap.ok,
          `Step ${step.id} invalid FEN (${step.fen}): ${snap.ok === "error" ? snap.message : ""}`,
        ).toBe("snapshot");
        if (snap.ok !== "snapshot") continue;

        expect(snap.outcome.type, `Step ${step.id} must be ongoing`).toBe("ongoing");
        expect(snap.legal_moves.length, `Step ${step.id} has no legal moves`).toBeGreaterThan(0);

        for (const moveTok of step.acceptedMoves) {
          const matchesLegal = snap.legal_moves.some(
            (m) =>
              m.long_notation === moveTok ||
              m.notation === moveTok ||
              m.short_notation === moveTok,
          );
          expect(
            matchesLegal,
            `Step ${step.id} acceptedMove (${moveTok}) not found in legal_moves`,
          ).toBe(true);

          const applied = await callWasm({
            cmd: "apply_move",
            fen: snap.fen,
            move: moveTok,
            history_hashes: [snap.hash],
          });
          expect(
            applied.ok,
            `Step ${step.id} apply_move (${moveTok}) failed: ${
              applied.ok === "error" ? applied.message : ""
            }`,
          ).toBe("move_applied");
        }
      }
    }
  });

  it("verifies the No-Majority step offers multiple legal capture choices", async () => {
    const noMajorityLesson = BASICS_LESSONS.find((l) => l.id === "no-majority");
    expect(noMajorityLesson).toBeDefined();
    if (!noMajorityLesson) return;

    const step = noMajorityLesson.steps[0];
    const snap = await callWasm({ cmd: "inspect", fen: step.fen });
    expect(snap.ok).toBe("snapshot");
    if (snap.ok !== "snapshot") return;

    expect(snap.legal_moves.length).toBeGreaterThanOrEqual(2);
    expect(snap.has_capture).toBe(true);
    expect(snap.legal_moves.every((m) => m.captures.length > 0)).toBe(true);
  });

  it("verifies the Promotion lesson steps produce a king piece after applying acceptedMoves", async () => {
    const promotionLesson = BASICS_LESSONS.find((l) => l.id === "promotion");
    expect(promotionLesson).toBeDefined();
    if (!promotionLesson) return;

    for (const step of promotionLesson.steps) {
      for (const moveTok of step.acceptedMoves) {
        const applied = await callWasm({
          cmd: "apply_move",
          fen: step.fen,
          move: moveTok,
        });
        expect(applied.ok).toBe("move_applied");
        if (applied.ok !== "move_applied") continue;

        expect(applied.applied.promotes).toBe(true);
        const hasLightKing = applied.snapshot.pieces.some(
          (p) => p.color === "light" && p.kind === "king",
        );
        expect(hasLightKing, `Step ${step.id} move ${moveTok} should produce a king`).toBe(true);
      }
    }
  });
});
