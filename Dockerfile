# Multi-stage Dockerfile for Limakhos (WASM engine + Vite frontend + Rust Axum server)

FROM rust:1-bookworm AS rust-builder
WORKDIR /app

RUN rustup target add wasm32-unknown-unknown

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build -p makhos-wasm --target wasm32-unknown-unknown --release \
 && cargo build -p makhos-server --release


FROM node:22-bookworm-slim AS web-builder
WORKDIR /app/web

COPY web/package.json web/package-lock.json ./
RUN npm ci

COPY web ./
COPY --from=rust-builder /app/target/wasm32-unknown-unknown/release/makhos_wasm.wasm ./public/wasm/makhos_wasm.wasm

ENV SKIP_WASM_BUILD=1
RUN npm run build


FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /data

COPY --from=rust-builder /app/target/release/makhos-server /usr/local/bin/makhos-server
COPY --from=web-builder /app/web/dist /app/web/dist

ENV LIMAKHOS_BIND=0.0.0.0:8080 \
    LIMAKHOS_DB=/data/limakhos.db \
    LIMAKHOS_STATIC_DIR=/app/web/dist \
    RUST_LOG=makhos_server=info,tower_http=info

VOLUME ["/data"]
EXPOSE 8080

CMD ["makhos-server"]
