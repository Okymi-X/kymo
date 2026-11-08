# Tasks

## Done
- [x] P1 server foundation: migration 0015 (event, user, player_team, player_team_member, event_participant, team_invite, token.user_id; tournaments backfilled into events and linked by event_id; old activity table renamed from `event` to `activity`), injectable clock in AppState, player accounts (register, player-login, /me, ban, registration switch), extractors `Player` and `Require<needs::X>` on kymo_core::Role::can, events API (public list/show with effective_state, admin CRUD, state machine, participants), solo and team registration, teams (create, join by code, leave, kick, rotate code, member-only reads), admin users. 11 new integration tests in tests/players.rs, existing 40 untouched. Open: team_invite table unused, no per-IP limit on register, tournament state is not synced to its event after creation, web UI (P6)
- [x] Bracket as a map: bounded viewport, drag to pan with momentum, Ctrl+wheel and pinch zoom at the cursor, wheel pans (and releases to the page at the edges), arrows and +/-/0/F, minimap with click to jump, Show everything, Reset, Locate my team; drags never open a match. Pure math tested (9), e2e for drag, zoom, keys, reset, fit and no accidental clicks
- [x] Back navigation: back link on every sub page, header back button on phones; nav bar with live badge, wide search, light/dark switch (remembered), scroll shadow
- [x] Match scheduling (time and place), activity entries, calendar feed (.ics), follow chip countdown (engine 2, API 1, schedule tests 9, e2e 1)
- [x] Embeddable view (/embed/{slug}, iframe snippet dialog, transparent for stream overlays, only embed pages can be framed), print styles, 100 team demo tournament (CTF World Open)
- [x] Stage mode (/t/{slug}/stage): projector screen with a rotating bracket, standings or groups (15 s, pause, arrows, F for full screen), Up next, live Latest, champion takeover; Stage mode link on every started tournament. 1 e2e (17 total)
- [x] Navigation rework: primary link with active state, organizer menu (Appearance, API tokens, sign out, keyboard friendly), tournament title and progress move into the header when the page title scrolls away. Activity tab (public timeline with corrections), follow a team (remembered in the browser, chip with next match, highlighted in the bracket). 3 e2e
- [x] Demo data: `kymo-server seed-demo`, POST /admin/demo and a Load sample tournaments button; eight tournaments plus three Jeopardy events with challenges, hints, players and solves, safe to run again
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

## Done (latest)
- [x] Event countdowns show days/hours/minutes/seconds, stay compact, turn yellow under five minutes and red under two; after ending they show elapsed time at coarse human intervals
- [x] Ended event elapsed labels abbreviate hours, minutes, days and months while spelling out years
- [x] Right-align the countdown on event cards and event detail pages
- [x] Use a smaller, lighter countdown on event cards while retaining the larger event page version
- [x] Docker development override layers on production Compose; it keeps the same bind address, database path and volume, while cargo-watch rebuilds the embedded UI and restarts the server
- [x] Auth sessions are shared across tabs for organizer, player, monitor and referee sign-ins; cross-tab storage changes update the UI, and legacy per-tab sessions migrate on load
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

- [x] Second UX audit: organizer contact setting (GET/PUT /admin/contact, public in GET /registration) shown on event and sign in pages, live events shown as cards on an empty home page, every event card ends with View event, no duplicate Sign in in the phone menu, far future dates read No deadline, first column of wide tables stays put, Operations section links and a separated maintenance block, local VPN address labelled

- [x] Third audit (browser E2E): correct flags masked in the staff submission list with an audited Reveal (GET /admin/events/{id}/submissions/{sid}/value), admin Create account (POST /admin/users, temporary password shown once), failed sign in reads Username or password is incorrect, instance table names owner, team and challenge, Rank label, one Profile heading, Mark as playing keeps the dialog open, download confirmations, loud warning when the VPN address is loopback, contact field no longer shown twice

- [x] Retest fixes: Users page text points to Create account, roster refusals name the event and why (plus notes on the event page), a failed board refresh no longer closes an open challenge (test waits for the real 20 second refresh), a tournament's event follows its status (migration 0021), VPN lab owners are named, solved instance policy written down
- [ ] Deployment: set KYMO_VPN_HOST to a reachable address and run a real two machine OpenVPN test (the one open release blocker)

- [x] Compact phone (320px): dialog header actions drop under the title on a phone, palette help row wraps, page header fits; browser test checks pages and dialog geometry at 320px

- [x] Match result dialog: score first, live row that does not wrap, forfeit and schedule folded into sections (schedule opens when set), buttons never wrap their label
- [x] Test deployment at https://thiebou.okymi.org (Caddy in a container on 80/443 with the old csoc-report certificate volume, Kymo as a systemd service on 127.0.0.1:8080, no instances or VPN)

- [x] Score over time chart on the event scoreboard (GET /events/{slug}/scoreboard/timeline, kymo_jeopardy::progression, web ScoreChart)
- [x] CI fixed (a flaky cargo test was a real SQLite locking bug, see MEMORY) and now also runs fmt and the browser tests; release workflow publishes a tarball on a v* tag

- [x] Separate lab host: kymo-lab crate (server, binary, RemoteRuntime client with certificate pinning), installer and systemd unit, manifests, admin API and Lab hosts page, per event choice, Operations health, README guide and API reference, release ships a lab tarball. Proven on two machines (local platform in `labs` mode, remote Debian lab host installed by the installer)

## Roadmap (ordered by priority, from docs/PROMPT.md and the platform objective)

1. [x] Configurable permissions, step 1: admin editable role/permission matrix for the built-in roles (player, referee, monitor), stored in the `setting` table as differences from the defaults, enforced by every extractor (including the player routes through `Account<P>`), audited, with an admin page, a `GET /permissions/mine` for the web, and a reset to defaults. Admin and visitor stay locked so nobody can lock themselves out. Tests: kymo-core 9, tests/permissions.rs 8, web 4, e2e 2 (written, not run in this environment)
2. [ ] (not started) Configurable permissions, step 2: custom roles (name, description, own permission set, assignable to accounts), finer grained permissions registered by later features (participants, tournaments, tournament challenges), per event role grants, and web gating driven by the caller's permissions instead of role names.
3. [x] PROMPT P0, part 1: write the tournament challenge rules (assignment per match, round or pool; manual or automatic; challenge count for solo and team; match completion, ties, team submissions, DFIR partial credit) into ARCHITECTURE.md and get them agreed before any code.
4. [x] PROMPT P0, part 2: built (kymo-duel crate, migration 0028, `duel.rs`/`duel_store.rs`/`routes/duel.rs`, engine `set_result_tiebreak`, tournament challenge settings, DFIR items, manual and drawn assignment, answers with partial credit, finalize/restart, roster fill from registrations, player match view, organizer match section, tests: kymo-duel 5, engine 1, tests/duel.rs 1). Not done and written down: instance-backed challenges cannot be assigned to matches yet (authoring works, runtime start in a match does not), and event export/import still covers Jeopardy events only, so tournament challenges are not in the bundle yet
5. [x] PROMPT P1: event state and schedule consistency (migration 0029: registration policy and organizer switch, early start flag, reopen, frozen archive, schedule validation, paused shown on tournaments, bracket saves no longer undo pause/end/registration; rules in ARCHITECTURE; kymo-core policy boundary test). Server and browser boundary tests for these paths are not written yet
6. [x] PROMPT P1: admin managed participants (POST/DELETE /admin/events/{id}/participants, GET .../candidates; solo adds an account, team adds an existing team without touching its members; mode, duplicate and team-size checks; audited; refused for ended or archived events; removal keeps all history, ends running instances and is spelled out in the confirm dialog). Works for Jeopardy and tournament events alike, solo and team (tests/participants.rs, 2 tests)
7. [x] PROMPT P1: tournament administration moved into /admin/events/:id (new Tournament tab: rename, color, duplicate, backup, reset, delete, roster and start; Challenges tab and settings for challenge matches); /t/:slug keeps viewing, player matches, referee and admin scoring and shows a Manage tournament link for admins; create and import land on the admin page; event and tournament name and description are kept in step both ways. Browser specs for rename, color and duplicate were updated but not run here. Left on /t/:slug: Embed, Stage mode, calendar, bracket image (public tools), and score entry
8. [x] PROMPT P2: Ladder points-over-time graph (`GET /ladder/timeline`, `LadderChart`): team selection, readable totals, tooltips, and the same data as a table; history rules and the by-name identity rule are written in ARCHITECTURE and on the page
9. [~] Gaps from the feature review. Done: scoreboard freeze (migration 0030, setting on the event, banner, staff see live, lifts at the end) and Jeopardy scoreboard CSV. Still open, in this order: per instance dynamic flags, announcements, password reset by email, 2FA for staff, Prometheus metrics, OpenAPI spec

## Todo
- [x] Virtual machines on lab hosts (QEMU/KVM, OVA and disk images, overlays, readiness from QEMU, console screenshots), proven with VulnHub's Gaara and doubletrouble through a platform
- [x] Lab hosts: the VPN gateway on a lab host (per host authority, remote gateway driven over the lab API, VPN-only lab hosts, VM ports relayed onto the lab network, installer builds the gateway and opens the lab-to-lab rule, UI per host). Proven with a real OpenVPN client and a container on a rootful engine; the VM path through the tunnel is proven up to the host firewall (see MEMORY) and needs one allow rule to finish
- [x] Machines are machines: a tap on the lab bridge with DHCP, every port open on the address, manifest `display` machine/service, address-only player view; audit with IP for player instance and VPN actions, searchable paged audit viewer, wording and 320px fixes from the retest
- [ ] Lab hosts: several machines per instance on a private network; multi disk images; Windows guest notes
- [ ] Run a VM through a real tunnel end to end once the host firewall allows lab to lab traffic on the VM ports (the installer's rule, or the one in MEMORY)
- [ ] Lab hosts: per event limits per player/team are the platform policy today; a per lab capacity view and instance quotas per event
- [x] Real VPN end to end on the test server (rootful Docker): tunnel, private instance, isolation between players
- [ ] RUN `crates/kymo-vpn/tests/gateway.rs` against a daemon and fix what it finds. Its only real run failed at "the tunnel did not come up" before its logging existed, and later runs were blocked by the permission classifier in this session. Command in README (Private VPN, Verify a setup yourself). Likely cause: the test's client containers need `--disable-dco` (added, not yet run)
- [ ] Measure how many labs one gateway handles (each lab is one interface on the gateway container)
- [ ] Per instance random flag (so a leaked flag is not valid for others) and a readiness check before an instance is reported running
- [ ] Scoreboard import for seeding (first provider TBD), then per match result pull, see ARCHITECTURE
- [x] Sample data seed command (`seed-demo`): tournaments and Jeopardy events with challenges, hints, players and solves, idempotent
- [x] Public event cards use two columns on desktop and have more padding and larger titles
- [ ] README with quickstart
