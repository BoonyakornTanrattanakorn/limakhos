<img src="assets/icon.svg" width="64" height="64" alt="">

# Limakhos (หมากฮอส 8 ตัว)

A fast, rule-accurate **Thai Draughts (Makhos 8 — Thailand Chess Association B.E. 2568 / 2025)** web platform powered by a Rust bitboard core, in-browser WebAssembly analysis engine, and a lightweight self-hosted multiplayer server.

## Features

- **TCA 2025 Makhos 8 Rules**: 1–32 numbering, main diagonal (4–29), mandatory captures without majority rule, immediate removal, flying King halt-capture, promotion stops turn, automatic threefold repetition (Art. 13), automatic 16-move King-only draw (Art. 14/16), no mutual draw offers (Art. 15), and strict timeout loss (Art. 21).
- **100% Offline Browser Mode**: Local Analysis (Multi-PV + eval bar + SVG arrows), Play vs AI (Levels 1–8), Local 2P pass-and-play, and FEN/PDN import & export via `makhos-wasm` Web Worker.
- **Self-Hosted Multiplayer (`makhos-server`)**: Real-time WebSocket rooms, `12+5` / `5+3` / `3+2` / unlimited matchmaking and private invite links (`?room=<id>`), authoritative millisecond server clocks, live spectators, SQLite WAL game/PDN archiving, Argon2id accounts, and Glicko-2 ratings.

---

## (a) Local Development

### Prerequisites
- Rust toolchain with `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- Node.js 20+ (22 recommended)

### Run Tests & Build
```bash
# Run all Rust unit + integration tests (core, engine, wasm, server)
cargo test --workspace

# Run frontend unit tests and build WASM + Vite bundle
cd web
npm ci
npm test
npm run build
```

### Run Dev Servers (Hot Reload)
In terminal 1 (backend API + WebSockets on `:8080`):
```bash
cargo run -p makhos-server
```
In terminal 2 (Vite dev server on `:5173`, proxying `/api` and `/ws` to `:8080`):
```bash
cd web
npm run dev
```
Open `http://localhost:5173`.

### Server Environment Variables
- `LIMAKHOS_BIND` (default `0.0.0.0:8080`)
- `LIMAKHOS_DB` (default `data/limakhos.db`)
- `LIMAKHOS_STATIC_DIR` (default `web/dist`)

---

## (b) Self-Hosted Homeserver (Docker Compose + Cloudflare Tunnel)

Run the single-binary backend + built frontend locally on a mini-PC, NAS, or VPS with persistent SQLite WAL storage in `./data`:

```bash
docker compose up -d --build
```
The app is immediately available at `http://localhost:8080`.

To expose your homeserver safely on a custom domain with HTTPS/WSS using a free **Cloudflare Tunnel** (no open router ports required):

1. Create a tunnel in Cloudflare Zero Trust Dashboard and point a public hostname (e.g. `makhos.yourdomain.com`) to `http://limakhos:8080`.
2. Export your tunnel token (or put it in `.env`) and start both services:
```bash
CLOUDFLARE_TUNNEL_TOKEN=<your-tunnel-token> docker compose --profile tunnel up -d --build
```

---

## (c) Hybrid Deployment: Cloudflare Pages Frontend + Homeserver Backend

You can host the static frontend globally on **Cloudflare Pages** (so Analysis / AI / 2P modes load from the edge and work offline) while pointing the **Online** tab to your self-hosted backend behind a Cloudflare Tunnel:

1. Expose your `makhos-server` container on an API hostname (e.g., `https://api.makhos.yourdomain.com`).
2. Build the frontend on Cloudflare Pages with:
   - **Build command**: `cd web && npm ci && npm run build` (or install Rust `wasm32-unknown-unknown` in the build environment)
   - **Output directory**: `web/dist`
   - **Environment variable**:
     ```
     VITE_API_BASE=https://api.makhos.yourdomain.com
     ```
   The frontend automatically maps `https://` to `wss://` for `/ws/room/:id` and sends CORS-enabled requests to `/api/*`.
