# syntax=docker/dockerfile:1.7

FROM rust:1.88-slim-bookworm AS builder

ARG DIOXUS_CLI_VERSION=0.7.10

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends binaryen ca-certificates curl libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
RUN cargo install dioxus-cli --version ${DIOXUS_CLI_VERSION} --locked

COPY . .

ARG TODO_SERVER_URL=http://127.0.0.1:3000
ENV TODO_SERVER_URL=${TODO_SERVER_URL}
# D4d's second service. Baked in at build time like the first, because `platform::USER_BASE_URL`
# reads it through `option_env!` — a wasm bundle has no environment to read at run time.
ARG USER_SERVER_URL=http://127.0.0.1:3001
ENV USER_SERVER_URL=${USER_SERVER_URL}

WORKDIR /app/examples/todo-app
RUN dx build --web --release --locked \
    && test -f /app/target/dx/todo-app/release/web/public/index.html

FROM nginx:1.27-alpine AS runtime

COPY examples/todo-app/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/target/dx/todo-app/release/web/public/ /usr/share/nginx/html/

EXPOSE 80
