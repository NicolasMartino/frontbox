# syntax=docker/dockerfile:1.7

FROM rust:1.88-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libsqlite3-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release --locked -p user-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/user-server /usr/local/bin/user-server

ENV USER_SERVER_ADDR=0.0.0.0:3001
EXPOSE 3001

HEALTHCHECK --interval=5s --timeout=2s --start-period=2s --retries=12 \
    CMD curl -fsS http://127.0.0.1:3001/api/v1/users > /dev/null || exit 1

CMD ["user-server"]
