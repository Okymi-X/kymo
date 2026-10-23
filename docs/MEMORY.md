# Memory

- 2026-10-23: Decisions: Rust (Axum + SQLx) backend, React frontend, SQLite default, v1 scope is knockout core only. Inspired by evroon/bracket.
- Toolchain on host: cargo 1.99, node 22, pnpm 12.
- Engine: matches use next_win/next_lose pointers; byes are Slot::Bye and auto-resolve; grand reset is void (Bye,Bye) unless the losers champion wins the grand final. set_result/clear_result work on a clone, so errors leave the bracket untouched.
- Server: env KYMO_ADMIN_PASSWORD (min 12 chars, required), KYMO_DATABASE_URL, KYMO_ADDR. Sessions persist in the token table (7 days). Login has no rate limit yet. sqlx used without compile time macros so no DATABASE_URL is needed to build.
- Dev: port 8080 is taken on this host, run the server with KYMO_ADDR=127.0.0.1:8787 and the web dev server with KYMO_API=http://127.0.0.1:8787 pnpm dev. Do not use pkill -f with the binary path inside the same command line, it kills the shell.
- Never use window.confirm or alert in the web app: they freeze browser automation. Use ConfirmDialog.
- Login throttle is one shared budget, not per IP: an attacker can lock the organizer out for a minute. Revisit with per IP limits once a trusted proxy header is configured.
- API token secrets are shown once at creation; only the sha256 hash is stored.
