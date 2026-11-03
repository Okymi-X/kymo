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
### Platform API (phase 1)
Auth: POST /auth/register {username,password,email?} (201, returns token and user), POST /auth/player-login, GET|PUT /me (display_name, bio, current_password + new_password), POST /auth/logout works for player sessions too. Roles: player, referee, monitor, admin; extractors Player and Require<needs::ManageEvents|ManageUsers|ManageSettings|Monitor|Referee> (auth.rs) decide through kymo_core::Role::can. Legacy admin password and API tokens act as admin.
Events: public GET /events (drafts and archived hidden), GET /events/{slug} (404 for drafts; `state` stored, `effective_state` from kymo_core at server time). Admin: GET|POST /admin/events, PUT|DELETE /admin/events/{id} (delete only draft or archived, mode locked once someone registered), POST /admin/events/{id}/state (409 on an invalid move), GET /admin/events/{id}/participants.
Registration: POST|DELETE /events/{slug}/register (Player; solo registers the user, team mode registers the caller's team, owner only).
Teams (Player): POST /teams, GET /teams/mine, POST /teams/join {invite_code}, GET /teams/{id} (members only, else 404), POST /teams/{id}/leave, POST /teams/{id}/invite-code/rotate, DELETE /teams/{id}/members/{user_id}.
Admin users: GET /admin/users?q&page&per_page, PUT /admin/users/{id} {role?, banned?}, GET|PUT /admin/registration {open}.
Tables (migration 0015): event, user, player_team, player_team_member, event_participant, team_invite, token.user_id, tournament.event_id; the old tournament activity log is now `activity`. Tournaments responses gained `event_id`.

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

## Platform architecture (v2 plan)
Crates: kymo-engine (brackets, pure), kymo-core (event lifecycle state machine, time windows, role permissions, pure), kymo-jeopardy (flag matching, static and dynamic scoring, hint penalties, ranking with tie-breakers, submission rate math, pure), kymo-instances (InstanceRuntime trait, lifecycle state machine, Docker runtime via bollard, in memory fake for tests), kymo-server (HTTP, persistence, auth, monitoring, wiring).
Data model additions: event(id, kind tournament|jeopardy, mode solo|team, state, registration/start/end times, scoreboard_public), user(id, username, email, password hash, role, banned), team and team_member (global, joined to events through event_participant), challenge, challenge_release, hint, attachment, submission (append only), solve, instance, request_log, rate_rule, ban, audit. Existing tournaments are linked to an event row by migration; behavior does not change.
Auth: players sign in with username and password, session tokens in the token table with a user id; existing admin password and API tokens keep working. Roles are checked by extractors (Player, Referee, Admin, Monitor), never in handlers by hand.
Rules stay in the pure crates; the server only loads data, calls them and persists in one transaction.
Phases: 1 core + accounts + events + teams; 2 jeopardy engine + challenge CRUD + submissions + scoreboard; 3 instances; 4 monitoring + bans + audit; 5 ops (health, backup/restore, cleanup); 6 web UI for each phase.

## Instances (phase 3)
`kymo-instances`: `InstanceSpec` (image, port, cpu_millis, memory_mb, pids, ttl_secs, env, read_only) is stored as the challenge `instance_config` and validated on save and again at start against `ResourceCaps` (image charset, no leading dash, env names). `policy::authorize` decides Start, Restart, Stop from the latest run, a start budget (default 5 per challenge, failed starts are free), a restart cooldown (30 s) and one live instance per participant across challenges. A live run past `expires_at` reads as expired; expired runs cannot be restarted or stopped, a new Start creates a fresh run.
Server: `instance` table, one row per run. A start inserts a `starting` row under the write lock, releases it, then calls the runtime, so slow image pulls never block other writes. The reaper (`instances::reap`, every 30 s, also POST /admin/instances/reap) ends runs that are past expiry plus 5 s, belong to an ended or archived event, or a retired challenge, and removes labelled containers no live row owns.
Docker runtime: one bridge network per instance (`kymo-<run id>`), cap_drop ALL, no-new-privileges, memory, swap, CPU and pids limits, optional read only rootfs, random published host port, labels `kymo.managed` and `kymo.instance`. Enable with KYMO_INSTANCES=docker, KYMO_INSTANCE_HOST (shown to players), KYMO_INSTANCE_BIND (default 0.0.0.0).
API: GET|POST|DELETE /events/{slug}/challenges/{id}/instance, POST .../instance/restart, admin GET /admin/events/{id}/instances, DELETE /admin/instances/{id}, POST /admin/instances/reap. Container handles, image and environment are never returned to players.

## Monitoring (phase 4)
`kymo-monitor` is pure: `Rule` (endpoint `*`, `/path`, `/prefix/*`, optional method; max requests per window; ip or user scope; strikes to ban; ban seconds, 0 = permanent), `Limiter` (sliding window per rule and subject, denials within 10 minutes are strikes, enough strikes returns Ban and resets the counters), ban expiry and subject validation.
Server: `monitor::layer` wraps the whole /api/v1 router. Order per request: identify the bearer token, skip everything for staff (anyone with the Monitor permission, so a bad rule cannot lock organizers out), reject banned addresses or users, apply matching rules (429 with Retry-After, or an automatic ban with a `ban` row, an audit entry and a cache update), run the handler, then write one `request_log` row (matched route pattern, path without query, status, address, user id, milliseconds; never query strings, bodies or tokens). Rules are matched against the matched route pattern, for example `POST /api/v1/auth/player-login`, so path parameters never fragment a rule. Unmatched paths are logged as `unmatched`. Monitor reads are not logged.
Rules and active bans are cached in memory, loaded lazily, and reloaded after every change through the API; the limiter state is in memory only. Address 0.0.0.0 (peer unknown) is throttled but never banned. Behind a reverse proxy set KYMO_TRUST_PROXY=1 so the last X-Forwarded-For entry is the client; without it every client shares the proxy address and an automatic ban would hit everyone. `request_log` rows older than 7 days are deleted every 10 minutes.
Permissions: Monitor (read log, summary, rules, bans, submission history), ModerateBans (create and change rules, create and lift bans), ViewAudit. A moderator cannot ban themselves, their own address, or any account that holds the Monitor permission. User bans also set `user.banned` and delete the user's sessions.
Seeded rules (migration 0018): player login 30/min and register 20/min per address with a 15 minute ban after 10 denials, and a catch all of 1200/min per address with no ban.

## Operations (phase 5)
Health: GET /api/v1/health is public and only says ok or down (503); it is not logged. GET /admin/ops/health (Monitor permission) returns database ping and size, 15 minute traffic (requests, 5xx, 429, average ms), instance counts (live, expired but not yet reaped, orphans, runtime reachable), the last result of the reaper, backup and maintenance jobs, each open event with participants, live instances and submissions in the last 10 minutes, and the last 20 5xx requests. Status is `degraded` when the database or runtime is unreachable or the reaper or backup last failed.
Backups: GET /admin/ops/backup/download streams a `VACUUM INTO` snapshot (consistent while serving; organizer only, never cached, audited; it holds password and session hashes). With KYMO_BACKUP_DIR the maintenance task (every 10 minutes) writes `kymo-<unix time>.db` when the newest is older than KYMO_BACKUP_EVERY_HOURS (24), keeps KYMO_BACKUP_KEEP (7), directory 0700, files 0600. POST /admin/ops/backups writes one now. SQLite writes nothing for an in memory database, so snapshots refuse to report success without a file.
Restore is offline by design: `kymo-server restore <backup.db> [--force]` opens the backup read only, requires `PRAGMA integrity_check` = ok, the migrations table, the first-migration tables, and a schema no newer than the binary; it refuses while a non empty `-wal` exists unless forced, keeps the current file as `<db>.pre-restore-<unix time>`, and swaps via a staging file. Older backups are migrated forward on the next start.
Event bundles (`kymo-event` v1): GET /admin/events/{id}/export and POST /admin/events/import (ManageChallenges, up to 256 MB). A bundle has the event settings, categories, challenges with flags, hints, attachments (base64 with sha256) and Jeopardy settings; not players, teams, solves or submissions (the snapshot has those). Import validates everything first, creates a new draft event (slug gets a numeric suffix if taken), and deletes the event again if a later write fails. By default the schedule is cleared and every challenge is a draft; `keep_times` and `keep_state` restore them.
Cleanup (POST /admin/ops/cleanup and the maintenance task): instance reaper, request log rows older than 7 days, expired sessions.

## Private VPN per player (phase 7)
Event setting `network_mode`: `published` (a host port per instance, as before) or `vpn`. Changing it needs no running instance and the VPN to be configured.
`kymo-vpn`: `addressing` (a lab pool, default 10.77.0.0/16 split into /27 networks, and a tunnel pool, default 10.8.0.0/16, both addressed by an integer index so the database stores only indexes), `pki` (own CA with P-256 keys via rcgen: one certificate per account with keyUsage and extended key usage as OpenVPN's remote-cert-tls demands, a signed revocation list, the tls-crypt static key), `profile` (server config and the `.ovpn`, with host and name validation because both land in config files), `rules` (the firewall script built from typed addresses only), `gateway` (trait plus a fake and a Docker implementation).
Data: `vpn_client` (one identity per user, `idx` never reused), `vpn_revoked` (serials), `vpn_lab` (one network per scoring participant per event), `instance.lab_ip`. The CA, the gateway certificate, the tls-crypt key and the CRL counters are `setting` rows, so one database backup restores every issued profile.
Gateway: one container (`kymo-vpn-gateway`, alpine with openvpn and iptables, NET_ADMIN and /dev/net/tun, all other capabilities dropped) on the default network with the UDP port published. It mounts the VPN directory read only at /vpn: `server.conf`, `pki/`, `ccd/<cn>` (one `ifconfig-push` file per account, with `ccd-exclusive` so a missing file means no connection) and `rules.sh`. Every client is pushed one route, the whole lab pool; what a client can reach is decided only by the firewall: FORWARD policy DROP and, per (player, lab), an ACCEPT in each direction in the KYMO_FWD chain, ending in DROP. Tunnel to lab traffic is masqueraded so lab containers need no return route. No `client-to-client`, no `redirect-gateway`.
Labs: `ensure_lab` makes an internal bridge network (no route out) with the pool's subnet, `isolate=true` on Podman; instances join with a fixed address and nothing published. The gateway joins every lab (`attach`). A lab is created on the first VPN instance of a participant and removed 10 minutes after the last one stopped, or at once when the event has ended.
`vpn::sync` is the single reconcile step and is idempotent: it writes the files (atomically, secrets 0600), reissues the gateway certificate within 30 days of expiry, writes the CRL when the revoked set changed, rewrites address files and removes those of banned or revoked accounts (kicking their session through the management console), ensures labs and attachments, rewrites `rules.sh`, and runs it. It runs at start, after a profile download, a revocation, a ban or unban, a team join, leave or kick, before an instance starts in a lab, and from the maintenance task every 10 minutes. Failures are logged and never fail the player's own action, except starting an instance, which needs the firewall to be right.
API: GET /vpn, POST /vpn/profile (a new certificate each time, the previous serial revoked in the same transaction, `Cache-Control: no-store`, seeded rate limit 10 per hour per user), GET /admin/vpn, POST /admin/vpn/sync, DELETE /admin/vpn/clients/{user_id}, PUT /admin/events/{id}/network. Health gains a `vpn` section and a dead gateway makes the server `degraded`.
Verification, honestly: the pure rules and the PKI by unit tests (certificates parsed with x509-parser) and by OpenSSL itself (`tests/openssl_interop.rs`: chain, client and server purposes, the revocation list, key pairing); the generated server config and a generated profile load in the host's OpenVPN 2.7.8 up to the point where it needs privileges to open the tunnel; the server flows against fakes (`crates/kymo-server/tests/vpn.rs`, 15 tests). The design itself, two players isolated through one gateway with revocation, was shown by hand with plain containers on rootless Podman (reach own lab, blocked from the other, blocked from the other tunnel address, refused after the address file is removed). The Rust driven end to end test (`crates/kymo-vpn/tests/gateway.rs`) is written and compiles, but it has NOT yet passed on this machine: its one real run failed at "the tunnel did not come up" and later runs were blocked in this environment. Run it before trusting a deployment (command in the README).

## Instance readiness and addresses (phase 8)
A run is `starting` from the moment its row is reserved. When the container exists the run keeps its handle and port but stays `starting`; only a successful connection check turns it `running`, and only then does the API send an address. The check is `reachable(host_port)` on the runtime for a published port (a TCP connect to the bind address) and `reachable(ip, port)` on the gateway for a private lab, run inside the gateway container with `nc -w 2 IP PORT </dev/null` because the gateway is attached to every lab. `start` waits up to 3 seconds so a quick service is running in the same response; otherwise the page polls.
`refresh` settles a live run and is called from GET, from `start` and from the reaper: a `starting` run that answers becomes `running`, one that has not answered after 60 seconds is stopped and marked failed (`never_ready`), a run whose container exited or vanished is stopped and marked failed (`exited`), a start that died is `start_failed`. Failed runs do not use the start budget. `instance.failure` holds the reason (migration 0020) and is sent to the page.
Lab addresses are handed out in turn (`free_lab_ip`): the slot after the most recent one used in that lab, skipping live ones, so a new instance never reuses the address of the one that just ended. Each player's lab is its own /27, so two players never share an address; inside one lab the first slot is simply the first address handed out.
GET /events/{slug}/instances lists a player's live instances with the challenge title, address and time left.


## Instance policy after a solve
A solved challenge no longer starts new instances (the player or team already has the points). Restarting solved challenges without scoring is a possible later feature, not current behaviour.
