# Project Kickoff & Technical Handoff: **Limakhos**

**Project Goal:** Build a fast, open-source, self-hosted web platform dedicated exclusively to **8-Piece Thai Draughts (หมากฮอส 8 ตัว / Makhos 8)**, modeled on the clean UX of Lichess and Lidraughts—featuring real-time online play, Glicko-2 ratings, client-side WASM engine analysis, AI sparring, an Opening Explorer, Endgame Tablebases, and Tactical Puzzles (*Kon Makhos*).

---

## 1. Architecture, Tech Stack & Hosting Strategy

Rather than forking the full `RoepStoep/lidraughts` stack (which inherits Lichess's multi-service Scala/Play, `lila-ws`, MongoDB, Redis, and 10×10-centric complexity), Limakhos will be built as a **lean, Makhos-only monorepo** inspired by Lichess/Lidraughts UX. Using a single shared **Rust** core for both the server and browser WebAssembly eliminates duplicated rules logic and keeps resource usage tiny for homeserver hosting.

### Tech Stack

| Layer | Technology | Role & Scope |
| --- | --- | --- |
| **Shared Rules & AI Engine** | **Rust** (`crates/makhos-core`, `crates/makhos-engine`) | Single source of truth for Makhos rules, legal move generation, PDN/FEN, 32-bit bitboard search, and tablebase probing. Compiles **natively** for the server and to **WASM (`wasm32`)** for the browser. |
| **Backend Server** | **Rust / Axum** (`crates/makhos-server`) | Single binary handling HTTP API, native WebSockets for real-time games & clocks, matchmaking, Glicko-2 ratings, game archives, and local auth. In-memory live game state (no Redis required). |
| **Database** | **SQLite (WAL mode)** | Embedded, zero-ops local storage on the homeserver for users, PDN game archives, ratings, puzzles, and opening trees. Simple single-file backups. |
| **Frontend Board & Web App** | **TypeScript + Vite** (`web/`) | Clean SPA with a **purpose-built lightweight 8×8 Makhos board component** (no `draughtsground` legacy overhead): `1–32` official TCA square numbering, step-by-step multi-jump selection & animation, arrows/annotations, mobile touch ergonomics, and Thai/English `i18n`. |
| **Client-Side Analysis & AI** | **Rust → WebAssembly** (single-threaded WASM) | Runs directly in the user's browser for live analysis and AI sparring (Levels 1–8) with **zero server CPU cost** and no external bot daemons. |

### Self-Hosted & Zero-External-Service Deployment

Limakhos has **no hard dependencies on external APIs, cloud databases, or third-party auth/engine services**—everything runs locally on a homeserver (via Docker Compose or a single systemd service).

* **Pure Homeserver + Cloudflare Tunnel (Recommended):**
  * Run the `makhos-server` binary (serving both the API/WebSockets and static frontend) + SQLite + static tablebase files locally on the homeserver.
  * Expose it on your custom domain via **Cloudflare Tunnel (`cloudflared`)** (free)—providing automatic HTTPS/WSS TLS and zero open router ports.
* **Hybrid Option — Cloudflare Pages (Frontend) + Homeserver (Backend):**
  * **Cloudflare Pages** (free) can host the static frontend (`web/`), WASM engine binary, and static tablebase chunks on Cloudflare's global CDN for fast page loads.
  * **Note on Cloudflare Pages backend limits:** Cloudflare Pages only runs static assets and stateless serverless Functions—it cannot directly host a persistent stateful WebSocket server + SQLite database without paid/metered Workers/Durable Objects. Therefore, the stateful backend (`api.<yourdomain>`) stays on your **homeserver** behind Cloudflare Tunnel.
* **Licensing:** Because both the server/engine and the 8×8 board UI are written cleanly from scratch, the project has full license freedom (e.g., MIT or Apache-2.0) with no inherited AGPL-3.0 / GPL-3.0 obligations.

---

## 2. Core Game Logic Specification (`crates/makhos-core`)

Canonical reference: **[Thai Draughts (Makhos 8) Competition Rules — Thailand Chess Association B.E. 2568 (2025)](./Thai%20Draughts%20%28Makhos%208%29%20Competition%20Rules%20-%20Thailand%20Chess%20Association%202025.md)**. Do **not** inherit International, Russian, or Brazilian capture rules—Makhos 8 has distinct mechanics:

* **Board, Numbering & Initial Setup (Articles 1–4):**
  * 8×8 board using **32 dark playable squares** numbered `1–32`.
  * **Light moves first** and occupies squares **`1–8`** (Light's bottom-left dark square is **`4`**, bottom-right is **`1`**; Light advances upward toward `29–32`).
  * **Dark moves second** and occupies squares **`25–32`** (Dark's bottom-left dark square is **`29`**, bottom-right is **`32`**; Dark advances upward toward `1–4`).
  * **Main diagonal:** `4–29` (bottom-left to top-right from both perspectives). Middle 4 ranks (`9–24`) start empty.
* **Man (*Bia* / เบี้ย) Movement & Capture (Articles 5–6):**
  * Moves **1 square diagonally forward** only (no backward moves).
  * Captures **diagonally forward only** (no backward captures by Men), jumping over an adjacent opponent piece onto the empty square immediately behind it.
* **King (*Hot* / ฮอส) Movement & Capture (Articles 5–6):**
  * **Flying Movement:** Moves any number of vacant squares diagonally forward or backward along all 4 diagonal directions.
  * **King Halt Capture:** Can initiate a jump from any distance along a diagonal, and **always lands on the immediate empty square directly behind the captured piece** (never slides further past the jumped piece on that jump). From that landing square, if another capture is available along any diagonal, the King may fly to execute the next jump.
* **Capture Priority & Multi-Jumps (Articles 5–6):**
  * Captures are **mandatory**.
  * **No Majority Rule:** Players do *not* have to choose the maximum-capture line. Any valid capture path (e.g., 2-piece vs. 3-piece, Man vs. King) is legal, provided all consecutive jumps along the chosen path are completed.
  * **Immediate Removal:** Jumped pieces are removed from the board *immediately* mid-sequence (preventing double-jumping the same piece, and allowing a flying King to pass back across the newly emptied square during the same multi-jump if another capture is available).
  * **Promotion:** A Man reaching the opponent's back rank (`29–32` for Light, `1–4` for Dark) via move or jump promotes immediately to a King and **ends the turn** (cannot continue capturing as a King on the same turn).
* **Win / Draw Conditions (Articles 11–16):**
  * **Win:** Opponent has 0 pieces remaining, 0 legal moves remaining (stalemate = loss), resigns, or runs out of time on the clock (Article 21: flag loses regardless of remaining material).
  * **Threefold Repetition (Article 13):** Automatically declared a draw when the same board position (including side to move) occurs 3 times.
  * **Automatic 16-Move Rule (Articles 14 & 16):** Automatically declared a draw if **16 full moves (32 plies)** occur with **no capture and no Man move** (i.e., King-only maneuvering). Any capture or Man move by either side resets the counter to `0`.
  * **No Mutual Draw Agreement (Article 15):** Draws occur *only* via threefold repetition or the 16-move rule—there is no draw-offer button in official/rated play.
* **Move Notation, PDN & Time Controls (Articles 43–47):**
  * **Notation:** Quiet moves use `start-end` (`8-11`), captures use `startxend` (`5x14`). Official display/export short form for multi-captures writes only start and final square (`5x30`), while internal state & PDN disambiguation preserve the full step sequence (`5x14x23x30`) whenever two legal capture paths share the same start and end squares.
  * **Official Default Time Controls:** **Standard** `12+5`, **Rapid** `5+3`, **Blitz** `3+2` (plus optional casual custom clocks and paired 2-game match support in tournaments per Article 1).

---

## 3. Engine, Tablebases & Content Deliverables

### A. Bitboard AI Engine & Endgame Tablebases (`crates/makhos-engine`)

* **Bitboard Representation:** Because Makhos uses 32 playable squares, compact board states and tablebase keys fit cleanly into `uint32_t` bitmasks (`light_men`, `dark_men`, `kings`). Move generation internals may use padded bitmasks in `uint64_t` to keep diagonal shifts branchless and fast (~10M+ nodes/sec native, near-native in WASM).
* **WASM Build Strategy:** Target **single-threaded WASM** by default (already very fast on 32-bit bitboards, avoids `COOP`/`COEP` header constraints, and works seamlessly on both homeserver and Cloudflare Pages).
* **Engine Verification & Calibration:**
  * **Perft & Rules Parity:** Automated `perft` test suite covering tricky multi-jumps, 180° King reversals, promotion stops, 16-move draws, and stalemate wins—shared directly between server and WASM since both use `makhos-core`.
  * **Benchmarking:** Use `51616/CU_Makhos` and `witchu/alphazero` as reference baselines, plus an internal **Hub-protocol self-play match runner** (SPRT / Elo gauntlet) to tune evaluation and calibrate AI Levels 1–8.
* **Endgame Tablebases:** Generate complete retrograde Win/Draw/Loss (**WDL**) and Distance-to-Zero (**DTZ**, calibrated to the exact **16-full-move / 32-ply** draw rule) tablebases offline for all **4-piece and 5-piece endgames**. Host the generated files statically on the homeserver (and/or Cloudflare Pages) for instant lookup in analysis without external services.

### B. Opening Explorer, Endgames & Puzzle Database

* **Opening Book (`เปิดหมาก`):** Seed initial explorer with canonical Thai formations (split by First Mover / Light vs. Second Mover / Dark):
  * *สามตัวเรียง* (Three-in-a-Row), *อีปุ้ม* (E-Poom), *มาตรฐาน* (Standard), *กองโจร* (Guerrilla), *ดาวกระจาย* (Starburst), *หนีบข้าง* (Flank Pinch).
* **Tactical Puzzles (`กลหมากฮอส` / Kon Makhos):**
  * Digitize classical public-domain / original composed Thai tactical studies (multi-piece forced sacrifices/stalemate traps) into PDN format—verify content provenance/licensing before importing third-party collections.
  * Run local engine self-play mining to extract positions with a single winning move (`eval swing >= +4.0`) for rated puzzle training.
* **Interactive Practice Modules:** Guided tutorials for **2 Kings vs. 1 King**, **3 Kings vs. 1 King**, **Sacrificial Breakthroughs (ต่อเข้าฮอส)**, and **Main Diagonal Control (`4–29`)**.

---

## 4. Phased Execution Roadmap

Structured to ship a **playable, self-hosted MVP** early before layering on tablebases and the full study/content suite.

### Phase 1: Makhos Core, Local Board & Engine Foundation (Weeks 1–3)

* [ ] Initialize monorepo (`crates/makhos-core`, `crates/makhos-engine`, `crates/makhos-server`, `web/`) and CI checks (`cargo test`, `wasm-pack`, TypeScript build).
* [ ] Implement complete TCA 2025 Makhos 8 rules in `makhos-core` (Light-first `1–8` numbering, King Halt capture, no-majority capture, immediate removal, promotion stop, threefold repetition, automatic 16-full-move / 32-ply draw counter, PDN/FEN).
* [ ] Write comprehensive unit & `perft` test suites verifying every rulebook diagram (Articles 4–6) and edge cases (multi-jumps, 180° King reversals, stalemate wins, disambiguated multi-capture notation).
* [ ] Build purpose-built 8×8 Makhos board UI in `web/` (optional `1–32` numbering overlay from both Light and Dark perspectives, drag/click + multi-jump animations, arrows, mobile touch ergonomics, Thai/English `i18n`).
* [ ] Implement initial Rust alpha-beta bitboard engine + **WASM build** for local browser play (AI Levels 1–8) and client-side analysis board.

### Phase 2: Self-Hosted Multiplayer MVP (Weeks 4–6)

* [ ] Build `makhos-server` (Axum + WebSockets + SQLite WAL): local account auth, matchmaking lobby with official presets (`12+5`, `5+3`, `3+2`), real-time game loop & clocks, Glicko-2 ratings, and PDN game archive.
* [ ] Integrate client-side WASM analysis board for post-game review (eval bar, best-move arrows, mistake/blunder highlights).
* [ ] Package homeserver deployment (Docker Compose / systemd, automated SQLite backups, basic rate limiting & move-timing telemetry).
* [ ] Configure domain routing via **Cloudflare Tunnel** (and optionally deploy `web/` + WASM static bundle to **Cloudflare Pages**).
* [ ] Run closed alpha testing with local players (WebSocket stability, reconnection handling, clock sync, mobile touch).

### Phase 3: Tablebases, Study Suite & Public Beta (Weeks 7–10)

* [ ] Generate 4-piece & 5-piece WDL/DTZ endgame tablebases locally (using the 32-ply draw rule) and wire static lookup into the analysis board and engine.
* [ ] Import ~200+ canonical Thai opening lines and build the local Opening Explorer (backed by SQLite/static index + player game stats).
* [ ] Curate initial 500+ *Kon Makhos* tactical puzzles (classical studies + engine-mined puzzles) with rated puzzle mode.
* [ ] Build interactive Practice & Learn modules (Rules basics, *2 ฮอส ไล่ 1 ฮอส*, sacrificial breakthroughs, main diagonal control).
* [ ] Load-test homeserver concurrency, verify backup/restore drills, and launch Public Beta.

---

## 5. Immediate Next Steps

1. **Scaffold Monorepo:** Initialize the Rust Cargo workspace (`makhos-core`, `makhos-engine`, `makhos-server`) + Vite TypeScript app (`web/`) and verify native + `wasm32` compilation.
2. **Implement & Test `makhos-core`:** Encode the TCA 2025 board numbering (`1–8` Light first, `25–32` Dark) and encode every diagram from [Thai Draughts (Makhos 8) Competition Rules — 2025](./Thai%20Draughts%20%28Makhos%208%29%20Competition%20Rules%20-%20Thailand%20Chess%20Association%202025.md) as regression unit tests.
3. **Homeserver Baseline:** Confirm Docker + `cloudflared` tunnel setup on the homeserver for staging deploys with your domain.
