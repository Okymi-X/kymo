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
RUN cargo build --release -p kymo-server

FROM debian:stable-slim
RUN useradd --system --create-home --home-dir /data kymo
COPY --from=server /app/target/release/kymo-server /usr/local/bin/kymo-server
USER kymo
WORKDIR /data
ENV KYMO_ADDR=0.0.0.0:8080 KYMO_DATABASE_URL=sqlite:///data/kymo.db
EXPOSE 8080
VOLUME /data
CMD ["kymo-server"]
