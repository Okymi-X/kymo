# Architecture

## Stack
- Backend: Rust, Axum, SQLx (SQLite default), tokio, tower-http, argon2, serde.
- Frontend: React 19, TypeScript, Vite, Tailwind v4, TanStack Query, dnd-kit, Motion.
- Frontend is built to static files and embedded into the server binary (rust-embed).

## Layout
```
crates/kymo-engine   pure bracket logic, no IO, no async, fully unit tested
crates/kymo-server   HTTP API, persistence, auth, SSE, static serving
web/                 React app (admin + public view)
docs/
```

## Boundaries
- kymo-engine owns formats: bracket generation, winner advancement, loser routing, byes, validation of edits. It operates on plain data (Bracket, Match, Slot).
- kymo-server loads a tournament, calls the engine, persists the result in one transaction. Handlers never contain bracket rules.
- Persistence sits behind a repository module so Postgres can be added later without touching handlers.
- web talks only to `/api/v1`. Types are mirrored by hand in `web/src/api/types.ts` until an OpenAPI step is added.

## Data model
tournament(id, name, slug unique, format, status draft|live|done, bracket JSON, created_at)
team(id, tournament_id, name, seed, score)
setting(key, value JSON)  one row per settings group, today only `appearance`
token(id, hash sha256, kind session|api, label, created_at, expires_at)
The bracket is one JSON document owned by the engine, saved whole after each edit. A process wide write lock serializes read-modify-write cycles.

## API (/api/v1)
Public: GET /settings, GET /tournaments, GET /tournaments/{slug}, GET /tournaments/{slug}/events (SSE), POST /auth/login.
Admin (Bearer token, session or API token): PUT|DELETE /admin/settings, GET|POST /admin/tokens, DELETE /admin/tokens/{id}, POST /tournaments, PUT|DELETE /admin/tournaments/{id}, PUT .../teams, POST .../start, POST .../reset, PUT|DELETE .../matches/{match_id}/result.

## Data flow
Organizer action -> API handler -> repository loads matches -> engine applies change -> repository saves in a transaction -> SSE event -> public clients refetch.

## Key decisions
- Rust engine as a library crate so it can later compile to WASM for client side preview.
- SQLite first for zero config self hosting; SQLx queries kept portable.
- Matches store pointers to the next match (winner and loser), so propagation is a graph walk, not formula math.

## Customization
Appearance (site name, accent per color scheme, corner radius, typeface) lives in the `setting` table and is served at GET /settings. The web app injects it as a CSS variable override on load; every value is validated on the server (hex colors, radius 0 to 20, fixed font list) and again before it reaches CSS. New options extend the JSON, no migration. Contrast against the page background is shown while editing.

## Challenge integration (planned, not built)
Kymo schedules and displays knockout rounds; it does not host challenges. A round can later carry a `challenge` reference with a `kind` (flag, web, full pwn, kubernetes lab) and a provider adapter (CTFd, rCTF, HTB or custom webhook) that reports who solved what. Adapters return plain results the engine can turn into scores, so the bracket logic stays unaware of challenge types. Decision pending: pull data from third party platforms versus accept pushed results through API tokens (tokens already exist).
