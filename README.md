# Kymo

Open source knockout brackets for CTF tournaments. Rust API, React UI, SQLite.

## Run in development

```sh
export KYMO_ADMIN_PASSWORD='at least 12 characters'
KYMO_ADDR=127.0.0.1:8787 cargo run -p kymo-server
cd web && pnpm install && KYMO_API=http://127.0.0.1:8787 pnpm dev
```

Open the Vite URL, sign in as organizer, create a tournament, add teams, start.

## Test

```sh
cargo test && cargo clippy --all-targets -- -D warnings
cd web && pnpm exec tsc -b
```

Docs for contributors and agents live in `docs/`.
