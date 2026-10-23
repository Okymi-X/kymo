# Kymo

Open source knockout brackets for CTF tournaments. Rust API, React UI, SQLite.

## Run in development

```sh
export KYMO_ADMIN_PASSWORD='at least 12 characters'
KYMO_ADDR=127.0.0.1:8787 cargo run -p kymo-server
cd web && pnpm install && KYMO_API=http://127.0.0.1:8787 pnpm dev
```

Open the Vite URL, sign in as organizer, create a tournament, add teams, start.

## Run with Docker

```sh
KYMO_ADMIN_PASSWORD='at least 12 characters' docker compose up --build
```

The app serves the UI and API on port 8080. Data lives in the `kymo-data` volume.

## Test

```sh
cargo test && cargo clippy --all-targets -- -D warnings
cd web && pnpm exec tsc -b
```

Docs for contributors and agents live in `docs/`.

## End to end tests

```sh
cd web && pnpm e2e
```

Starts its own server on port 8791 with a throwaway database and drives the organizer flow in Google Chrome. Build the web app first (`pnpm build`).
