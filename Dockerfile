FROM node:22-slim AS web
RUN corepack enable
WORKDIR /app/web
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM rust:1-slim AS server
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY --from=web /app/web/dist web/dist
# --locked: the image is built from Cargo.lock exactly, as the release workflow does.
RUN cargo build --release --locked -p kymo-server

FROM debian:stable-slim
RUN useradd --system --create-home --home-dir /data kymo
COPY --from=server /app/target/release/kymo-server /usr/local/bin/kymo-server
USER kymo
WORKDIR /data
ENV KYMO_ADDR=0.0.0.0:8080 KYMO_DATABASE_URL=sqlite:///data/kymo.db
EXPOSE 8080
VOLUME /data
# GET /health answers 200 without a token. Plain bash does the request, so the image needs no curl.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n' >&3 && head -n 1 <&3 | grep -q ' 200 '"]
CMD ["kymo-server"]
