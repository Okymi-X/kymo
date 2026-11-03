# Tasks

## Done
- [x] P1 server foundation: migration 0015 (event, user, player_team, player_team_member, event_participant, team_invite, token.user_id; tournaments backfilled into events and linked by event_id; old activity table renamed from `event` to `activity`), injectable clock in AppState, player accounts (register, player-login, /me, ban, registration switch), extractors `Player` and `Require<needs::X>` on kymo_core::Role::can, events API (public list/show with effective_state, admin CRUD, state machine, participants), solo and team registration, teams (create, join by code, leave, kick, rotate code, member-only reads), admin users. 11 new integration tests in tests/players.rs, existing 40 untouched. Open: team_invite table unused, no per-IP limit on register, tournament state is not synced to its event after creation, web UI (P6)
- [x] Bracket as a map: bounded viewport, drag to pan with momentum, Ctrl+wheel and pinch zoom at the cursor, wheel pans (and releases to the page at the edges), arrows and +/-/0/F, minimap with click to jump, Show everything, Reset, Locate my team; drags never open a match. Pure math tested (9), e2e for drag, zoom, keys, reset, fit and no accidental clicks
- [x] Back navigation: back link on every sub page, header back button on phones; nav bar with live badge, wide search, light/dark switch (remembered), scroll shadow
- [x] Match scheduling (time and place), activity entries, calendar feed (.ics), follow chip countdown (engine 2, API 1, schedule tests 9, e2e 1)
- [x] Embeddable view (/embed/{slug}, iframe snippet dialog, transparent for stream overlays, only embed pages can be framed), print styles, 100 team demo tournament (CTF World Open)
- [x] Stage mode (/t/{slug}/stage): projector screen with a rotating bracket, standings or groups (15 s, pause, arrows, F for full screen), Up next, live Latest, champion takeover; Stage mode link on every started tournament. 1 e2e (17 total)
- [x] Navigation rework: primary link with active state, organizer menu (Appearance, API tokens, sign out, keyboard friendly), tournament title and progress move into the header when the page title scrolls away. Activity tab (public timeline with corrections), follow a team (remembered in the browser, chip with next match, highlighted in the bracket). 3 e2e
- [x] Demo data: `kymo-server seed-demo`, POST /admin/demo and a Load sample tournaments button, six tournaments covering every format and state (1 API test)
- [x] No flash on refresh: server inlines the saved look (CSS, data-scheme, title, JSON) into index.html, the app seeds its cache from it. Appearance page rebuilt: presets, brand and tagline, color mode, density, live preview with real components, sticky save bar. Server 2 tests, web 8 tests, e2e 1
- [x] Activity log API (events table, GET /tournaments/{slug}/activity, corrections marked), 1 test
- [x] Groups then knockout: snake seeded groups (2 to 8, 3 to 32 teams), top 1 to 4 advance, knockout fills when the groups finish and follows group edits until a knockout match is played, same group rematches avoided in round one. Engine 6 tests, API 1, stats 3, e2e 1. Groups view with a table per group and folded fixtures
- [x] Round robin: circle method schedule (3 to 32 teams), draws allowed, 3/1/0 points, table by points then goal difference then goals, champion when all matches are played; Standings with D and Pts, Schedule board by round, e2e (engine 7 tests, API 1, stats 4)
- [x] QA round 1 (Codex and a Claude subagent, 17 distinct findings, 2 not reproduced) fixed: per client login throttle with Retry-After, JSON error envelope for all API errors, hidden character and score validation, unicode slugs, SSE and static 404s, server side logout, roster editor state and start errors, standings ranking with third place, long name overflow, live site name preview. 4 new API tests, 3 new e2e, 1 new ranking test
- [x] Data views: standings, matches, teams, team dialog with path, hover highlight, CSV export, wide bracket area (12 stats tests, 1 e2e)
- [x] Design polish: responsive sticky header, PageTitle, depth tokens, dialog motion, fit to width bracket, larger touch targets, empty state
- [x] Home redesign: display type, status filter with counts, cards with avatar stack and match progress, skeletons, empty state. Engine progress() (2 tests), API progress field (1 test)
- [x] Appearance settings in the DB (GET /settings, admin PUT and reset, migration 0005, 1 API test), Appearance page with live preview and AA contrast check, 4 theme tests, e2e
- [x] Optional third place match for single elimination: engine (5 tests), tournament option (migration 0004, 1 API test), layout under the final (2 tests), create form checkbox, e2e
- [x] Team avatars via blobatar (deterministic from team name) on match cards, roster rows and the champion banner
- [x] Forfeit wins: engine (4 tests), PUT .../forfeit endpoint (1 test), dialog buttons, W/FF on the card, e2e test
- [x] Web: command palette (Ctrl/Cmd K, filter, arrows, enter), 4 unit tests and 1 e2e test
- [x] Repo published at github.com/Okymi-X/kymo, MIT license added
- [x] Playwright e2e (system Chrome): organizer plays a 4 team knockout to a champion, public view is read only, bad password rejected
- [x] API test plays a 7 team double elimination tournament to a champion over HTTP (7 API tests total)
- [x] Web: single canvas layout with links from winners and losers into the grand final (9 layout tests), verified in browser
- [x] Web: API token panel (create with one time secret, list, revoke via dialog), checked in browser
- [x] Server: sessions and API tokens stored hashed (sha256) in SQLite, survive restarts; /admin/tokens list, create, revoke; test added
- [x] Seeding by qualifier score (team score column, roster editor, API), confirm dialog replaces window.confirm
- [x] Server: login throttle (5 failures per minute shared budget, 429), test added
- [x] Web: bracket layout with SVG connectors and zoom (buttons, ctrl+wheel), layout unit tests (4)
- [x] Web embedded in the server binary (rust-embed), security headers, Dockerfile and compose (image builds and serves UI and API)
- [x] Web: tokens, shell, home, login, live bracket view (SSE), score dialog, drag and drop roster, start/reset/delete. Builds clean, bracket rendering checked in a browser screenshot
- [x] Server: SQLite, tournament/team/match endpoints, admin session auth, SSE (3 integration tests)
- [x] Docs initialized
- [x] Workspace scaffold, engine crate: single and double elimination, byes, edit, undo (11 tests, clippy clean)

## In progress
- [ ] P1 kymo-core crate: event state machine, time windows, permissions (pure, tested)
- [ ] P1 kymo-jeopardy crate: flags, scoring, ranking, rate math (pure, tested)

## Done (latest)
- [x] P8 experience pass: instances wait for the service to answer, show no address before that or after the container dies, fail with a reason, rotate addresses and are listed per event; one dialog framework with sticky header and footer, scroll lock, a discard guard and a redesigned challenge editor with live validation; the sign-up form explains closed sign-ups up front and validates as you type; plain language errors everywhere and no more generic token message; controls that respond at once; a race that signed a freshly signed in referee out is fixed. Evidence: Rust 310 tests, web unit 97, browser 42, clippy and tsc clean, and screenshots of the editor and the instance states taken against real containers
- [x] P7 private VPN per player, built and tested except one real container run: kymo-vpn crate (addressing 7 tests, PKI 7, profile 4, firewall rules 4, gateway fake 2, OpenSSL interop 1), lab networks and placement in kymo-instances (16 tests), migration 0019, server service and routes (tests/vpn.rs 15), event network mode, bundles carry it, health and ops page, web panels (player profile download, instance note, admin network mode, ops gateway section), e2e 40, README deployment and VPN guide, deploy/ examples (systemd, Caddy, nginx, env, gateway image)
- [x] P3 Docker runtime verified against a live daemon (rootless Podman through its Docker compatible socket): start, publish a port, serve, find by label, stop, no leftover container or network. Stop kills immediately (21 s to 0.9 s). tests/docker.rs in kymo-instances, skipped unless KYMO_TEST_IMAGE is set. Lab images in lab/. Full player flow with real containers checked on a throwaway server
- [x] P6 web UI: player accounts (sign in, create account, profile), events list and event page (register, challenge board, challenge dialog with hints, downloads, instance panel and flag form, scoreboard, my progress), team page, admin events (create, import, overview, challenges editor, participants, submissions, instances), users, operations, monitor (traffic, rate limits, bans, audit). Server: `registered` flag on GET /events/{slug}, request log keeps the original path. Evidence: web unit tests 87, e2e 39 (2 new: player solves a challenge to the scoreboard and the monitor, team create and join), server tests green, clippy clean, tsc clean
- [x] P5 operations: public GET /health, admin GET /admin/ops/health (database, traffic, instances and runtime, job status, per event activity, recent 5xx), consistent snapshot download (VACUUM INTO, organizer only, audited), scheduled backups with retention (KYMO_BACKUP_DIR, _EVERY_HOURS, _KEEP, private 0700/0600), offline `kymo-server restore <file> [--force]` with integrity, schema and Kymo checks that keeps the old file, per event bundle export/import (JSON with flags, hints, attachments, settings; validated whole, drafts by default, ?keep_times ?keep_state), cleanup (expired sessions, old log rows, instance reaper). tests/ops.rs 13, workspace green, clippy -D warnings clean, restore smoke tested on an old-schema database
- [x] P4 monitoring: kymo-monitor crate (rule matching, sliding window limiter with strike escalation, ban expiry; 12 tests), migration 0018, middleware on /api/v1 (ban check, per endpoint rules, request log), /admin/monitor/{requests,summary,rules,bans,audit,events/{id}/submissions}, audit entries for bans, rules, user changes, event state, instance force stop and auto bans, trusted proxy option, log pruning after 7 days. tests/monitor.rs 11 tests, workspace green, clippy -D warnings clean
- [x] P3 instances: kymo-instances crate (spec validation and caps, lifecycle policy, runtime trait, in memory fake, Docker runtime behind the `docker` feature), migration 0017, player start/restart/stop/show, admin list/force stop/reap, reaper task every 30 s (expired, ended event, retired challenge, orphan containers). Evidence: kymo-instances 15 tests, tests/instances.rs 11, workspace green, clippy -D warnings clean. Docker runtime compiled and clippy clean but NOT run against a daemon
- [x] P2 server Jeopardy (migration 0016): categories, challenges, hints, attachments, release/retire, admin test-flag without recording, submission history with filters, submit flow (time gate, duplicate 409, rate limit 429, attempt cap, bad format not counted), hint unlock cost, dynamic points recomputed from solve counts, scoreboard hidden until published, own and team progress with teammate breakdown. Evidence: jeopardy.rs 15 tests; workspace 40+45+41+4+40+15+11 tests, clippy -D warnings clean
- [x] Paginated Matches, Standings, Teams and Activity lists with search
- [x] Stage mode as rotating slides: current round cards, standings pages, bracket overview only when it fits at 0.55 zoom (32 team brackets)
- [x] Rename tournament, JSON backup, 404 page, shortcuts help (?), page tint and avatar toggle in Appearance

- [x] Logo upload (PNG, JPEG, WebP data URI under 60 KB, validated server side) and a tidier match result dialog

- [x] API tokens expire (7 days to 1 year or never) and API tokens cannot mint tokens

- [x] Public list trims rosters (team_count), drag announcements use team names, curved or square bracket lines, map fits under the header

- [x] Organizer tiebreak order (engine, PUT /admin/tournaments/{id}/tiebreak, Decide ties dialog) that also decides group qualifiers; grouped command palette

- [x] Swiss format (engine swiss.rs, rounds column, demo Swiss Open), palette search icon and key chips, select chevron fix

- [x] Per tournament accent color (Color button, scoped CSS), tournament import with schema, README quickstart

- [x] Duplicate tournament, import setup file, undo newest result, page titles, skip link

- [x] Codex QA round 2 fixes: strict import fields, activity search; stage screen fits one screen. Double elimination keeps a 3 team minimum on purpose (two teams is a single final)

- [x] Webhooks (Discord, Slack or any JSON URL) for results, corrections, forfeits, schedules and start/reset; host only is shown, secret path stays hidden

- [x] Referee tokens (scoped, results and schedules only, own sign in), planned time shown above the card, team path lines on round robin and Swiss maps

- [x] Best of 1, 3, 5 or 7 matches entered game by game (engine set_games, PUT .../games, score dialog, demo Grand Finals Bo5)

- [x] Late match panel with one click forfeits, chat link previews, captain result reporting with organizer approval (private link per team)

- [x] Ladder across tournaments, installable manifest, README feature list

- [x] Tournament description (rules, venue) shown under the title, in link previews and kept on duplicate and import

- [x] Bracket image download (PNG, SVG), Playing now marker with LIVE badge, activity line and chat message

- [x] UX audit fixes: phone menu replacing the history back arrow, "Search Kymo" palette with events and role pages, generic 404, sign in page wording, truthful registration messages (lib/platform.ts registrationStatus), Read more cue, Browse events on the empty home page, phone friendly Users/Monitor/Operations, scroll cue on wide tables, route level code splitting (entry 775 KB to 271 KB), request log written by a background batch writer

## Todo
- [ ] RUN `crates/kymo-vpn/tests/gateway.rs` against a daemon and fix what it finds. Its only real run failed at "the tunnel did not come up" before its logging existed, and later runs were blocked by the permission classifier in this session. Command in README (Private VPN, Verify a setup yourself). Likely cause: the test's client containers need `--disable-dco` (added, not yet run)
- [ ] Measure how many labs one gateway handles (each lab is one interface on the gateway container)
- [ ] Per instance random flag (so a leaked flag is not valid for others) and a readiness check before an instance is reported running
- [ ] Scoreboard import for seeding (first provider TBD), then per match result pull, see ARCHITECTURE
- [ ] Sample data seed command based on the schema
- [ ] README with quickstart
