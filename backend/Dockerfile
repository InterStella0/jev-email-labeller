# syntax=docker/dockerfile:1

# ---- build ----
FROM rust:1-alpine AS chef
RUN apk add --no-cache build-base && cargo install cargo-chef --locked
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /src/recipe.json recipe.json
# Dependencies are cached in this layer until Cargo.toml/Cargo.lock change.
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked && cp target/release/jev-labeller /jev-labeller

# ---- runtime ----
FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S -g 10001 app \
    && adduser -S -D -u 10001 -G app -h /app app \
    && mkdir -p /app/data \
    && chown app:app /app/data
WORKDIR /app
COPY --from=builder /jev-labeller /app/jev-labeller

USER app
ENV DATA_DIR=/app/data \
    BIND=0.0.0.0:8080 \
    RUST_LOG=info,jev_labeller=info
EXPOSE 8080
VOLUME ["/app/data"]

# The worker touches /app/data/heartbeat every 30s; stale for 3 minutes = unhealthy.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD ["/app/jev-labeller", "healthcheck"]

CMD ["/app/jev-labeller"]
