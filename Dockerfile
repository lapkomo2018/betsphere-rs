# Production image: multi-stage release build, no source or toolchain in the
# final image. BuildKit cache mounts keep dependency compilation incremental
# across builds (requires BuildKit, the default in modern Docker).

FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked && \
    cp target/release/betsphere /usr/local/bin/betsphere

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home appuser
COPY --from=builder /usr/local/bin/betsphere /usr/local/bin/betsphere
USER appuser
EXPOSE 8080
ENTRYPOINT ["betsphere"]
