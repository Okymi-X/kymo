# Memory

- 2026-10-23: Decisions: Rust (Axum + SQLx) backend, React frontend, SQLite default, v1 scope is knockout core only. Inspired by evroon/bracket.
- Toolchain on host: cargo 1.99, node 22, pnpm 12.
- Engine: matches use next_win/next_lose pointers; byes are Slot::Bye and auto-resolve; grand reset is void (Bye,Bye) unless the losers champion wins the grand final. set_result/clear_result work on a clone, so errors leave the bracket untouched.
- Server: env KYMO_ADMIN_PASSWORD (min 12 chars, required), KYMO_DATABASE_URL, KYMO_ADDR. Sessions persist in the token table (7 days). Login has no rate limit yet. sqlx used without compile time macros so no DATABASE_URL is needed to build.
- Dev: port 8080 is taken on this host, run the server with KYMO_ADDR=127.0.0.1:8787 and the web dev server with KYMO_API=http://127.0.0.1:8787 pnpm dev. Do not use pkill -f with the binary path inside the same command line, it kills the shell.
- Never use window.confirm or alert in the web app: they freeze browser automation. Use ConfirmDialog.
- Login throttle is per peer address (ConnectInfo). Behind a reverse proxy all clients share the proxy address, so the budget is global again. Add a trusted proxy header option before deploying behind one.
- API token secrets are shown once at creation; only the sha256 hash is stored.
- Start the local app with setsid nohup so it survives the tool shell: KYMO_ADMIN_PASSWORD=$(cat .kymo-password) KYMO_ADDR=127.0.0.1:8787 KYMO_DATABASE_URL=sqlite://kymo.db setsid nohup target/debug/kymo-server &
- e2e: pnpm e2e in web/ uses channel chrome (no browser download), port 8791, db target/e2e.db. Needs web/dist built first.
- Avatars come from the blobatar npm package (blobatarUri, data URI in an img). CSP img-src allows data:. Avatar depends on the team name, so renaming a team changes its avatar.
- Third place match: Stage::ThirdPlace is the last match; Bracket::champion uses the last Winners stage match for single elimination. The tournament is Done once the champion is known, even if third place is unplayed.
- References for challenge integration: GZCTF (AGPLv3, jeopardy plus A&D and KotH, Docker or Kubernetes) and ForcAD (GPLv3, A&D, rounds and flags). Integrate over HTTP only, never copy code.
- QA workflow: run a throwaway server (own port and db) in tmux, give testers the plan in scratchpad/qa/prompt.md. Codex reported a hover highlight bug that did not reproduce (4 rows lit on hover), check findings before fixing. Two testers on one server interfere through the login throttle and shared data.
- Framework rejections (bad JSON, 405, 413) are rewritten to {error} by the json_errors layer in routes/mod.rs.
