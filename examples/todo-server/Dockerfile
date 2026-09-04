# syntax=docker/dockerfile:1.7

FROM rust:1.88-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libsqlite3-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release --locked -p todo-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/todo-server /usr/local/bin/todo-server

ENV TODO_SERVER_ADDR=0.0.0.0:3000
EXPOSE 3000

HEALTHCHECK --interval=5s --timeout=2s --start-period=2s --retries=12 \
    CMD curl -fsS http://127.0.0.1:3000/api/v1/todos > /dev/null || exit 1

CMD ["todo-server"]
