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

## Round robin
`Format::RoundRobin` builds `Stage::League` matches with the circle method (an odd team count rests one team per round and creates no bye match). No advancement pointers. A scored tie is a draw (winner none); knockout stages still reject ties. `Bracket::standings()` ranks by points (3 win, 1 draw), goal difference, goals scored, then team id (ids follow seed order). The champion is the table leader once every match is played. Capped at 32 teams (496 matches). The web app swaps the Bracket tab for a Schedule board grouped by round.

## Groups then knockout
`Format::GroupKnockout` deals teams into 2 to 8 groups in a snake over the seeds, each group plays a round robin (`Match.group`), and a single elimination bracket of `groups * advance` qualifiers follows. The knockout is generated at start with empty first round slots (byes for missing seeds). `refresh_knockout` runs after every group result change: once every group match is played it seeds the qualifiers by group placement, then points, goal difference, goals scored and team id, swaps same group first round pairs apart when possible, and writes the slots through `set_slot`, so a played knockout match refuses a group edit that would change its teams. Ties on the last qualifying place are broken by the same ordering, not by a play-off. Options `groups` and `advance` live on the tournament row (migration 0006).

## Activity log
`event(id, tournament_id, at, kind, match_id, data)` is append only and public (GET /tournaments/{slug}/activity, newest first, 200 rows). Kinds: started, reset, result, corrected (a change to a match that already had a result), forfeit, cleared. `data` is a snapshot of the match after the change. Entries are written inside the same write lock as the bracket change, and a refused edit writes nothing.

## Scheduling and calendar
`Match.scheduled_at` (unix seconds) and `Match.venue` are plain data on the match, validated on write (2000 to 2100, 40 characters, no control characters), logged as `scheduled` events and never used by bracket logic. `GET /tournaments/{slug}/calendar.ics` exports every scheduled real match as a 45 minute VEVENT in UTC with RFC 5545 escaping and line folding.

## Security headers and embedding
A middleware sets nosniff, a same origin referrer policy and a CSP on every response. Everything gets `X-Frame-Options: DENY` and `frame-ancestors 'none'` except paths under `/embed/`, which get `frame-ancestors *`. Chrome treats `*` as network schemes only, so test iframes must be hosted on an http origin, and it blocks public looking hosts from framing localhost.

## Customization
Appearance (site name, tagline, accent per color scheme, corner radius, typeface, color mode, density) lives in the `setting` table and is served at GET /settings. The server also inlines the saved look into index.html (a `<style id="kymo-theme">`, `data-scheme` on `<html>`, the title and a JSON data block) so the first paint is already right and the app seeds its query cache from the block, which avoids a flash on refresh. The web app applies the same CSS through `applyAppearance`; every value is validated on the server (hex colors, radius 0 to 20, fixed font list) and again before it reaches CSS. New options extend the JSON, no migration. Contrast against the page background is shown while editing.

## Challenge integration (planned, not built)
Kymo is the tournament layer. It does not host challenges, run containers or check flags. Platforms that do (GZCTF, ForcAD, CTFd, rCTF, HTB) stay the source of truth for play, and Kymo turns their results into bracket results.

What the references show (read 2026-10-23):
- GZCTF: jeopardy with static or dynamic attachments and containers on Docker or Kubernetes, plus attack and defense and king of the hill modes. A&D ticks report Ok, Mumble, Offline or InternalError per team service. AGPLv3. Its public API and webhooks were not described on the pages read, check before designing an adapter.
- ForcAD: pure Python attack and defense framework. Rounds of `round_time` seconds, flags valid for `flag_lifetime` rounds, per team tokens, Hackerdom compatible checkers, scoreboard and admin panel. Only `/api/client/attack_data` is documented on the page read. GPLv3.

Design direction:
1. A `provider` adapter returns, for a given match, a score for each team (and optionally a status such as live, finished). Kymo maps that to `set_result`. The engine never learns about challenge kinds.
2. Three entry points, in order of effort: manual entry (done), push through API tokens (done, a script can PUT results), pull from a provider scoreboard on a timer.
3. A round may carry a `source` (provider, game id, challenge kind: flag, web, pwn, kubernetes, attack and defense) shown on the match card and in the dialog.
4. Qualifier import: a provider scoreboard fills `team.score` so seeding by score works with real data.
5. Integrate over HTTP only. Do not copy or link GPL or AGPL code into Kymo.
Open decision: pull versus push as the default, and which provider first. Recommendation: scoreboard import for seeding first, since it needs only read access and the score column already exists.
