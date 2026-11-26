# Limakhos — Phase-by-Phase Implementation Plan

This document tracks the stacked PR implementation plan for **Limakhos**, prioritizing the **local WASM engine analysis and playable 8×8 board** first, followed by the self-hosted multiplayer server and study/endgame suite.

Each phase is built step-by-step with atomic commits on its own branch, ends with a GitHub Pull Request, and each subsequent phase branches from the latest phase PR.

---

## Phase 1 — Core Rules, Bitboard Engine & WASM Core (`phase-1/core-and-engine` → `master`)

1. **Step 1.1 — Workspace & Docs Scaffolding:** Add kickoff/rulebook docs, implementation plan, `.gitignore`, and Rust Cargo workspace (`makhos-core`, `makhos-engine`, `makhos-wasm`).
2. **Step 1.2 — `makhos-core` (TCA 2025 Makhos 8 Rules):**
   - Exact `1–32` TCA coordinate system (Light moves first on `1–8`, Dark on `25–32`, main diagonal `4–29`).
   - 32-bit bitboard state (`light_men`, `dark_men`, `kings`) + Zobrist hashing.
   - Complete legal move generator: Man forward moves/captures, King flying moves & King Halt captures, mandatory captures without majority rule, immediate mid-sequence removal (including 180° King reversals), and promotion-ends-turn rule.
   - Win/loss/draw enforcement: 0 pieces / stalemate = loss, threefold repetition, automatic 16-full-move (32-ply) King-only draw counter, no mutual draw offers.
   - Official notation (`8-11`, `5x14`, short-form `5x30` + full-path disambiguation), FEN & PDN serialization/parsing.
   - Exhaustive unit tests encoding every TCA 2025 diagram (Articles 4–6, 44) plus `perft` benchmarks.
3. **Step 1.3 — `makhos-engine` (32-Bit Bitboard Alpha-Beta AI):**
   - Iterative deepening Negamax with alpha-beta pruning, transposition table, quiescence search, move ordering, and Principal Variation (PV) + Multi-PV extraction.
   - Makhos-specific evaluation: material, advancement, main diagonal (`4–29`) & center control, back-rank defense, mobility, endgame mop-up.
   - Calibrated AI difficulty Levels 1–8 + analysis metrics (score, mate-in-N, depth, nodes, NPS).
4. **Step 1.4 — `makhos-wasm` (Browser WebAssembly Module):**
   - Zero-tool-friction `cdylib` (`wasm32-unknown-unknown`) JSON bridge exposing rules, move validation, FEN/PDN import-export, Multi-PV engine analysis, and AI Levels 1–8 directly to browser Web Workers.

---

## Phase 2 — Playable 8×8 Board & Local WASM Analysis UI (`phase-2/playable-board-and-analysis` → `phase-1/core-and-engine`)

1. **Step 2.1 — Frontend Scaffold & Web Worker WASM Bridge:** Vite + TypeScript app (`web/`), WASM loader running off the main thread in a Web Worker, build script syncing `makhos_wasm.wasm`.
2. **Step 2.2 — Purpose-Built 8×8 Makhos Board Component:** TCA `1–32` numbering overlay (Light & Dark perspectives), flip board, click + drag-and-drop, step-by-step multi-jump path selection & animations, King visual stacking, SVG best-move/PV arrows.
3. **Step 2.3 — Play Modes & Live Engine Analysis Panel:**
   - Play vs. AI (Levels 1–8), Local 2-Player, and Free Analysis mode.
   - Official TCA clocks (`12+5`, `5+3`, `3+2`, Unlimited).
   - Live Lichess-style evaluation bar, Multi-PV candidate lines, depth/nodes/NPS readout, automatic `X / 16` draw counter, keyboard move navigation, PDN/FEN import & export, and Thai (`ไทย`) / English (`EN`) `i18n`.
4. **Step 2.4 — Frontend Unit Tests & Production Build Verification.**

---

## Phase 3 — Self-Hosted Multiplayer Server & Homeserver Deploy (`phase-3/self-hosted-server-mvp` → `phase-2/playable-board-and-analysis`)

1. **Step 3.1 — `makhos-server` Core & Persistence:** Rust Axum + SQLite (WAL mode), local username/password auth (Argon2 + signed session cookies), Glicko-2 rating engine, PDN archive storage.
2. **Step 3.2 — Real-Time Matchmaking & WebSockets:** Official `12+5` / `5+3` / `3+2` matchmaking queues, private challenge links, authoritative server clocks, reconnection recovery, and live spectator streams.
3. **Step 3.3 — Frontend Online Lobby & Live Play Integration:** Connect `web/` to `makhos-server` (while preserving 100% offline local play & WASM analysis when no server is connected).
4. **Step 3.4 — Self-Hosted Packaging:** Multi-stage `Dockerfile`, `docker-compose.yml` (with optional `cloudflared` Cloudflare Tunnel profile), Cloudflare Pages hybrid deployment guide, and server integration tests.

---

## Phase 4 — Endgame Tablebases, Opening Explorer & *Kon Makhos* Puzzles (`phase-4/tablebases-openings-puzzles` → `phase-3/self-hosted-server-mvp`)

1. **Step 4.1 — `makhos-tb` (4-Piece & 5-Piece Retrograde Tablebases):** Offline WDL/DTZ generator calibrated to the 32-ply TCA draw rule, compact static binary format, engine & server prober integration.
2. **Step 4.2 — Thai Opening Explorer (`เปิดหมาก`):** Canonical Thai opening book (*สามตัวเรียง*, *อีปุ้ม*, *มาตรฐาน*, *กองโจร*, *ดาวกระจาย*, *หนีบข้าง*) + Explorer UI drawer.
3. **Step 4.3 — *Kon Makhos* (`กลหมากฮอส`) Puzzles & Interactive Practice Modules:** Curated tactical studies, local engine puzzle-miner CLI, guided endgame practice (*2 ฮอส ไล่ 1 ฮอส*, *3 ฮอส ไล่ 1 ฮอส*, *ต่อเข้าฮอส*, main diagonal control), and interactive UI.
