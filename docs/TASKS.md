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



1. [x] Configurable permissions, step 1: admin editable role/permission matrix for the built-in roles (player, referee, monitor), stored in the `setting` table as differences from the defaults, enforced by every extractor (including the player routes through `Account<P>`), audited, with an admin page, a `GET /permissions/mine` for the web, and a reset to defaults. Admin and visitor stay locked so nobody can lock themselves out. Tests: kymo-core 9, tests/permissions.rs 8, web 4, e2e 2 (written, not run in this environment)
2. [ ] (not started) Configurable permissions, step 2: custom roles (name, description, own permission set, assignable to accounts), finer grained permissions registered by later features (participants, tournaments, tournament challenges), per event role grants, and web gating driven by the caller's permissions instead of role names.
3. [x] PROMPT P0, part 1: write the tournament challenge rules (assignment per match, round or pool; manual or automatic; challenge count for solo and team; match completion, ties, team submissions, DFIR partial credit) into ARCHITECTURE.md and get them agreed before any code.
4. [x] PROMPT P0, part 2: built (kymo-duel crate, migration 0029, `duel.rs`/`duel_store.rs`/`routes/duel.rs`, engine `set_result_tiebreak`, tournament challenge settings, DFIR items, manual and drawn assignment, answers with partial credit, finalize/restart, roster fill from registrations, player match view, organizer match section, tests: kymo-duel 5, engine 1, tests/duel.rs 1). Event export/import still covers Jeopardy events only, so tournament challenges are not in the bundle yet. Instance challenges were added to matches in the challenge parity step below
5. [x] PROMPT P1: event state and schedule consistency (migration 0030: registration policy and organizer switch, early start flag, reopen, frozen archive, schedule validation, paused shown on tournaments, bracket saves no longer undo pause/end/registration; rules in ARCHITECTURE; kymo-core policy boundary test). Server and browser boundary tests for these paths are not written yet
6. [x] PROMPT P1: admin managed participants (POST/DELETE /admin/events/{id}/participants, GET .../candidates; solo adds an account, team adds an existing team without touching its members; mode, duplicate and team-size checks; audited; refused for ended or archived events; removal keeps all history, ends running instances and is spelled out in the confirm dialog). Works for Jeopardy and tournament events alike, solo and team (tests/participants.rs, 2 tests)
7. [x] PROMPT P1: tournament administration moved into /admin/events/:id (new Tournament tab: rename, color, duplicate, backup, reset, delete, roster and start; Challenges tab and settings for challenge matches); /t/:slug keeps viewing, player matches, referee and admin scoring and shows a Manage tournament link for admins; create and import land on the admin page; event and tournament name and description are kept in step both ways. Browser specs for rename, color and duplicate were updated but not run here. Left on /t/:slug: Embed, Stage mode, calendar, bracket image (public tools), and score entry
8. [x] PROMPT P2: Ladder points-over-time graph (`GET /ladder/timeline`, `LadderChart`): team selection, readable totals, tooltips, and the same data as a table; history rules and the by-name identity rule are written in ARCHITECTURE and on the page
9. [~] Gaps from the feature review. Done: scoreboard freeze (migration 0031, setting on the event, banner, staff see live, lifts at the end) and Jeopardy scoreboard CSV. Still open, in this order: per instance dynamic flags, announcements, password reset by email, 2FA for staff, Prometheus metrics, OpenAPI spec

## Done (challenge parity)
- [x] The same challenge types in Jeopardy and tournament events (PROMPT updated): DFIR questions in Jeopardy with partial credit on the board (migration 0031), instance challenges and files in tournament matches, one authoring dialog (flag, instance, DFIR, files) for both, instance host and network mode for both, bundles keep DFIR. Tests: tests/dfir.rs. Not covered by a test: an instance started from a tournament match (needs a container runtime). Still Jeopardy-only: hints; tournament challenges are not in event bundles

## In progress (dynamic flags, multi flag, Responder and Verifier)
Spec: `docs/DYNAMIC_FLAGS_AND_RESPONDER.md` (Part A flags, Part B Responder). Dev toolchain: cargo runs in the `kymo-kymo` image, see MEMORY.
- [x] A1 `kymo-jeopardy::dynamic`: generate prefix + random hex + closing, shape validation, weighted point split, hint cost charge (11 tests)
- [x] A2 migration 0033 (flag_mode, multi_flag, responder, challenge_flag, instance_flag, flag_solve, submission.flag_id, instance.endpoints, instance.verifier_ip) and `challenge_store` (FlagSet validation, apply with the edit lock, labels only after a run or solve)
- [x] A3 admin API (`flag_mode`, `responder`, `flags` on create and update, flags in the list and detail) and event bundles (keys written only when used)
- [x] A4 instance start: `run_flags` generates the run's flags, `free_lab_ips` allocates the machine and Verifier addresses, flags and `verifier_ip` go to the lab host through `InstanceRuntime::start_with`, endpoints stored. Needs a lab host and, for a Responder, the VPN
- [x] A5 submission: single dynamic flag, several flags with partial credit, sharing alert (audit action `flag_shared`), hint cost taken from the awards in order, board pseudo challenges (FLAG_BASE), the multi flag challenge is solved with its last flag
- [x] A6 player views: `flags_total`, `flags_found`, `responder` on the challenge list, `flags_found/total` on the submit answer, `endpoints` on the instance view (10 integration tests in tests/dynamic_flags.rs)
- [x] B1 lab protocol and injection: `CreateRequest.flags`, `LabPlacement.verifier_ip`, `InstanceInfo.ip/endpoints`, manifest `inject` (env for containers, SSH files for VMs via russh, key preferred over password) and `verifier` blocks, `inject::plan_flags`, readiness gating (`pending`, status `starting`, `InstanceRuntime::ready`), version gate (workspace is now 0.4.0, platform refuses flags on older lab hosts). Tests: lab/tests/ssh.rs (7, real in process SSH server), lab/tests/flags.rs (6), inject unit tests (7)
- [x] B2 `kymo-verifier` crate (definition, runner for bash/python/exec, session, web service; 25 tests plus template tests) and the Verifier container on the lab host: `InstanceRuntime::start_verifier` (Docker: read-only root, no caps, definition mounted read-only, run description in the environment), `Record.verifier`, endpoints in the instance record, `GET /v1/instances/{id}/verify-log`, removal with the instance and orphan sweep, lab address validation. Kymo allocates `verifier_ip` like an instance address (tests/vpn.rs end to end: dynamic flag, Responder, refusals)
- [x] B3 `kymo-lab check` (inject and verifier problems, password file mode warning) and `kymo-lab verifier test DIR --target IP`, `deploy/verifier/Dockerfile`, installer (builds the Verifier image, tighter manifest modes, verifiers and keys directories, lab network firewall unit), release packaging, sample manifests (dynamic-flag, boot2root, responder), Verifier template and JSON Schema (`docs/schema/verifier.schema.json`)
- [x] C1 web: New challenge modal reordered (Basics, Instance, Flag(s), Points, Availability) with flag mode, number of flags, flag rows (name, label, prefix or value, random length, closing, weight, points share, example flag), Responder toggle and a copyable lab manifest block; `lib/flags.ts` (10 + 8 vitest); player flag box gets the `found/total` chip (theme colours) and per flag messages; instance panel shows Machine and Verifier rows; Instances tab Details dialog (run flags, audited, and Verifier runs); admin endpoints `GET /admin/instances/{id}/flags` and `/verify-log`
- [x] Player header status: a VPN pill (green Connected, grey Not connected, hidden until the player has a profile, polled every 5 s) and a pulsing instance icon (the player's one active instance, any event; opens its challenge via `/e/{slug}?challenge={id}`). Backend: `Gateway::connected` (OpenVPN management `status 2`), lab host `GET /v1/vpn/connected`, Kymo `GET /vpn/connection` (3 s cache per host) and `GET /me/instance`. The "Your private network" description text is gone. Tests: `tests/vpn.rs` (two), `parse_connected`
- [x] Docs: README sections (dynamic flags, several flags, Responder and Verifier), lab env example, spec marked implemented
- [x] Flag sharing rule per event (migration 0035, `sharing.rs`): submitting a flag generated for another player counts a strike; the first `sharing_warnings` (0 to 10, default 1) only warn, then the event's `sharing_action` applies: `off`, `manual` (audit `flag_sharing_review`) or `auto_ban` (default, via `monitor_alerts::ban_player`). The submit answer carries `sharing` (status warning, reported or banned), the challenge dialog shows it, and the Scoring settings form has the two fields. 2 integration tests in tests/dynamic_flags.rs; kymo-server suite green (demo and hundred team tests skipped), clippy and tsc clean. Not looked at in a browser
- [ ] Not run in this environment: Playwright e2e (needs Chrome), a real VM injection over SSH on a lab host, the Verifier image build and container start with a real Docker daemon, the firewall rule on a real host. The existing e2e selectors (`Flag` label exact) were kept
- [x] Containers that need a real login: manifest `cap_add` (allowlist of Docker's default capabilities) and `no_new_privileges`, applied per image by the Docker runtime (`kymo_instances::Hardening`), `kymo-lab check` warning, example manifest with comments, README section
- [x] Tournament challenges can use dynamic flags and Responder (server gate lifted, `duel::submit` judges against the side's own run through `submit::judge_dynamic`, editor shows Flag mode and Responder). Several flags in a match (migration 0036, `duel_answer.flag_id`, one unit per flag, a flag's weight is its points, default 1; the challenge is worth their sum; `flags` found/total in the match view; test `several_flags_score_one_by_one_in_a_match`). Ties keep the existing rule (first to reach the score, then the organizer decides). Sharing strikes are not applied in matches. Clippy clean, duel tests pass; not run in a browser
- [ ] Later: container file injection, QEMU guest agent method, dynamic and several flags in tournaments, more bundled languages for checks
- [x] Challenge editor: a Delete (trash) button next to Publish or Retire. Disabled with the reason for a published challenge or one with solves, a two step confirmation otherwise; the server rule (no solves) is unchanged (`web/src/pages/admin/ChallengeEditor.tsx`)
- [x] Optional challenge author (migration 0042, `author` on the admin and player challenge, in event bundles): an input in the challenge editor and a "Created by" line in the challenge dialog; empty means nobody is credited. The machine hint ("Every port is open: scan it, nmap ...") is removed from the instance panel
- [x] Challenge descriptions accept Markdown (headings, bold, italic, code, fenced blocks, lists, quotes, rules, http/https/mailto links). A small dependency free reader builds React elements, so no raw HTML, scripts or remote images (`web/src/lib/markdown.ts`, `components/Markdown.tsx`); a plain description looks as before
- [x] Browser tab icon on the Appearance page: the Kymo icon, the same image as the logo, or a separate upload (PNG, JPEG or WebP up to 60 KB, like the logo). Saved in the appearance row (`favicon_source`, `favicon`, no migration); the server puts it in the first paint and replaces the built-in links, and the page swaps it live

## Todo
- [x] Merge `origin/update` into `main` as 0.4.0, preserve challenge-level lab placement, reconcile migrations 0028 through 0034, and reinstall the platform, lab host and Verifier from the local release build
- [x] Register the Kymera appliance on the local lab host as VM artifact `kymera`; extract its compressed VMware disk, prepare the qcow2 base with KVM, and verify the lab service
- [x] Configure Kymera with two per-instance dynamic flags: inject `user` into `/opt/kymera/user.txt` for the Alpine container bind mount and `root` into `/root/root.txt` on the VM. Refreshed the lab image from the clean VMware disk and verified both paths through a disposable KVM instance
- [x] Reduce the Kymera appliance's unintended network exposure: bind Next.js to `127.0.0.1:3000`, keep nginx as the only public web entry point, and replace the TLS certificate with a minimal `kymera.ndh`-only subject/SAN. Rebuilt the VMware source and lab QCOW2, then verified ports, TLS metadata and dynamic flags on a disposable KVM instance
- [x] Fix the deployed VPN TLS timeout caused by two gateways claiming UDP 1194 on the same host: keep the lab-host gateway on 1194 and move the platform-local gateway to 1195. Recreated both containers and verified a player completed TLS 1.3, negotiated AES-256-GCM and received its fixed tunnel address
- [x] Virtual machines on lab hosts (QEMU/KVM, OVA and disk images, overlays, readiness from QEMU, console screenshots), proven with VulnHub's Gaara and doubletrouble through a platform
- [x] Lab hosts: the VPN gateway on a lab host (per host authority, remote gateway driven over the lab API, VPN-only lab hosts, VM ports relayed onto the lab network, installer builds the gateway and opens the lab-to-lab rule, UI per host). Proven with a real OpenVPN client and a container on a rootful engine; the VM path through the tunnel is proven up to the host firewall (see MEMORY) and needs one allow rule to finish
- [x] Machines are machines: a tap on the lab bridge with DHCP, every port open on the address, manifest `display` machine/service, address-only player view; audit with IP for player instance and VPN actions, searchable paged audit viewer, wording and 320px fixes from the retest
- [ ] Lab hosts: several machines per instance on a private network; multi disk images; Windows guest notes
- [ ] Run a VM through a real tunnel end to end once the host firewall allows lab to lab traffic on the VM ports (the installer's rule, or the one in MEMORY)
- [ ] Lab hosts: per event limits per player/team are the platform policy today; a per lab capacity view and instance quotas per event
- [x] Real VPN end to end on the test server (rootful Docker): tunnel, private instance, isolation between players
- [ ] RUN `crates/kymo-vpn/tests/gateway.rs` against a daemon and fix what it finds. Its only real run failed at "the tunnel did not come up" before its logging existed, and later runs were blocked by the permission classifier in this session. Command in README (Private VPN, Verify a setup yourself). Likely cause: the test's client containers need `--disable-dco` (added, not yet run)
- [ ] Measure how many labs one gateway handles (each lab is one interface on the gateway container)
- [~] Per instance random flag (so a leaked flag is not valid for others) and a readiness check before an instance is reported running (see In progress)
- [ ] Scoreboard import for seeding (first provider TBD), then per match result pull, see ARCHITECTURE
- [x] Sample data seed command (`seed-demo`): tournaments and Jeopardy events with challenges, hints, players and solves, idempotent
- [x] A challenge can run on its own lab host, overriding the event's (`PUT /admin/challenges/{id}/lab`, migration 0028, Runs on in the challenge editor)
- [x] Retest fixes: header fits 305 px, tap targets at least 32 px, Duplicate asks before copying
- [x] Public event cards use two columns on desktop and have more padding and larger titles
- [ ] README with quickstart

## Front page, sign in rule, public scoreboards and rules (2026-11-12)
- [x] `/` is the home page: a default page built from editable texts, or the organizer's own sanitized HTML and CSS (shadow root, no scripts, forms, frames, handlers or outside loads). Signed in players see a greeting, their running instance and their events. Admin editor on the Appearance page (`GET /home`, `PUT` and `DELETE /admin/home`)
- [x] Sign in is required for the whole API and every page except the allow list in `gate.rs` (sign in and up, `/settings`, `/home`, `/public/scoreboards`, team score reports, update pings, and the read only views of events with a public scoreboard). Anonymous answers are reused for 5 s
- [x] `/s/<event>` public scoreboard page; `GET /public/events/{slug}`; tournaments with a public scoreboard stay readable for embed, stage and calendar
- [x] Per event rules (`event_rules`, `GET /events/{slug}/rules`, `PUT /admin/events/{id}/rules`) in a dialog on the event page
- [x] Header and links: Tournaments entry removed, back links go to Events, tournament creation and samples moved to Manage events
- [x] Manage events has one create form: Type picks Jeopardy CTF (players, lab host) or Tournament (format, match, rounds, third place, Import setup); `TournamentCreate` is now `TournamentFields`, `ImportSetup`, `useCreateTournament`. The organizer e2e specs select Type first and use the "New event" label (not run: needs Chrome)
- [ ] Playwright specs still assume the old home page (tournament list and create form at `/`) and were not rerun; update them when e2e can run
- [x] Make the VPN port collision impossible to miss: `install-lab.sh` keeps 1194 on a separate machine, auto-picks the next free UDP port on a shared host (fresh install and re-run/upgrade, persisted to `lab.env`), and fails on an occupied explicit `--vpn-port`; `DockerGateway::ensure_running` now errors if the running gateway does not publish the configured UDP port. Unit, fmt, clippy and workspace check pass

## Monitor: live graphs and attack alerts (2026-11-12)
- [x] `kymo-monitor::detect` (pure): six detectors (scan, admin probe, attack strings, scanner tool, brute force, flood), per detector settings, percent decoding for double encoded payloads. 8 unit tests
- [x] Migration 0036: `request_log` gets query, user agent and a staff flag; tables `detector` (the organizer's changes only) and `alert`
- [x] `monitor_alerts.rs`: `run_once` every 30 s (alerts, auto-ban of signed in players at the ban level, expiry of temporary player bans), series, hot lists, heat map; routes under `/admin/monitor`; 6 integration tests in `tests/alerts.rs`
- [x] Web: Live and Alerts tabs on the Monitor page, SVG charts (no library), alert dialog, detection settings, badge in the account menu
- [x] Fixed: a temporary ban of a player never lifted the account's banned flag
- [ ] Players scanning the lab host from inside the VPN (needs lab host counters, separate step)
- [ ] Not looked at in a browser; no Playwright spec for the new tabs

## Organizer console, delivery-aware challenge editor, event readiness (2026-11-17)
- [x] Fixed after the merge: two migrations were numbered 0034; the monitor one is now `0036_monitor_alerts.sql` (the deployed 0.4.0 order 0028 to 0034 is unchanged, 0035 flag sharing, 0036 monitor)
- [x] Organizer door: `POST /admin/auth/login` (`routes/organizer.rs`, open in `gate.rs`): username + password for an account that is not a player, or an empty username for the organizer password. Wrong password, unknown name, banned and player account give the same 401; shares the IP throttle; every success is audited as `organizer_login`. Test in `tests/permissions.rs`
- [x] Web: the console lives under `/organizer` with its own shell (`components/organizer/`), sign in page (`/organizer/login`: Account, Password, Referee token), guard (`RequireStaff`, a player is told so instead of looping), overview, and a menu built from `/permissions/mine`. Old addresses (`/admin/*`, `/monitor`, `/settings`, `/tokens`, `/login`) redirect (`lib/organizer.ts`). The player header has one "Organizer console" entry for staff. e2e specs got the new sign in steps (not run)
- [x] Challenge editor split into `pages/admin/challenge/` (`draft.ts`, `DeliverySection`, `FlagRows`, `Extras`, `parts`). Delivery is chosen first: Flag only, Container (Docker or Podman), Virtual machine (OVA, qcow2, VMDK). A lab host's artifacts are listed with their runtime; a machine takes the manifest's CPU and memory; manifest flag names are suggested and a flag the manifest does not deliver is flagged. Pure rules in `lib/delivery.ts`
- [x] `GET /admin/lab-choices` and `GET /admin/lab-choices/{id}/artifacts` (needs only ManageChallenges, no addresses or tokens); `ArtifactInfo` gained `cpu_millis`, `memory_mb`, `pids`, `flags`, `verifier`, `display`
- [x] Event page tabs: Overview (state and "Before it goes live" checklist, `lib/readiness.ts`), Settings (details, scoreboard, rules), Challenges, Participants, Submissions, Infrastructure (host, network, flag sharing), Instances
- [ ] Not looked at in a browser; no Playwright spec for the console, the delivery cards or the checklist
- [ ] `InstanceHost` and `NetworkMode` still read `/admin/labs` (settings permission), so an event manager without it sees an empty host list

## Audit round: leaks, monitor, event views (2026-11-17)
- [x] Fixed: a signed in player could read a draft event's rules and summary (`/events/{slug}/rules`, `/public/events/{slug}`) and every page of a draft tournament (list, bracket, activity, calendar, update stream). One check, `auth::sees_drafts` (organizers and referees only); tests in `tests/gate.rs`, and `tests/api.rs` publishes its tournaments with `open_all` before reading as a player
- [x] Fixed: with instances switched off, the player instance list answered 503 and every polling page raised the monitor's "Server errors". It answers an empty list now; starting an instance still says why it cannot
- [x] Monitor: the request log has a `min_status` filter; the Server errors card opens the failing requests
- [x] Events list: filter by Live, Upcoming, Drafts, Finished with search (`lib/eventFilter.ts`), live events first on the player's Events page too; the challenge table has a Type column (`challengeType`); a running event shows "Right now" instead of the go-live checklist; the console overview lists what needs attention (open alerts, unreachable lab hosts)
- [x] Challenge editor: required fields stay quiet until Save was tried; machine hints no longer claim a manifest when none is known; a link to add a lab host
- [x] Reviewed and found sound: flag matching (anchored linear time regex, constant time compare, format mismatch is not an attempt), home page HTML/CSS sanitizer (every payload tried was removed or refused), attachment downloads (attachment disposition, nosniff, cleaned names), CSV export (formula characters escaped), response headers (CSP, frame, nosniff), SQL (parameters only; the two `format!` queries use constant table names)
- [ ] Coding challenges (submit code, run it against tests) do not exist. A judge executes untrusted code, so it needs its own sandbox design (no network, CPU/memory/time caps, one disposable container per run on a lab host); do not add it as a flag variant
- [ ] Patch challenges are the Responder with a Verifier (scripts written by the organizer); there is no diff or patch upload
- [ ] The demo data has no instance, multi-flag, dynamic-flag or Responder challenge (they need a container runtime), so those are covered by tests only

## Dynamic flags for every delivery, coding judge (2026-11-17)
- [x] Dynamic flags without an instance (migration 0037, `participant_flag`): a value is made for a participant the first time a template file is rendered for them and kept; checking (`flag_set::run_values`) and sharing detection (`owner_of_value`) read runs and participant flags together. The flag settings lock once any participant holds a flag
- [x] Template files: a text attachment uploaded with `?template=true` has `{{flag}}` / `{{flag:name}}` replaced per player at download (`kymo_jeopardy::template`, pure, tested). Checked at upload: challenge must be dynamic, file UTF-8 under 1 MB, every named flag must exist, at least one token. `GET /admin/attachments/{id}/preview` shows it with made-up flags. Bundles carry the `template` flag
- [x] Editor: dynamic mode for every delivery; a copy-ready template panel per delivery (flag-only file, container manifest and entrypoint, VM manifest with SSH delivery and command delivery, Responder verifier) in `lib/flagTemplates.ts`; go-live check blocks a dynamic, instance-less challenge with no template file
- [ ] Multi-question (DFIR) challenges have no dynamic answers: each question's answer is static. A question answer that names a flag (`{{flag:name}}`) would need flag rows on a DFIR challenge, a change to `run_item` and the item editor
- [x] Coding challenges (see below). Earlier note: `kymo-instances::oneshot` (`Job`, `JobRunner`, `FakeJobRunner`) and a hardened Docker runner (`DockerRuntime::run_job`: no network, read only root, all capabilities dropped, no-new-privileges, memory/CPU/pids/file size/open files limits, tmpfs, killed on time or output limit, always removed, orphans reaped by label) exist and compile; the judge itself (manifest `judge` block, `kymo-lab` endpoint, challenge kind `code`, player and editor views) is not written yet

## Coding challenges, lab host clarity, views (2026-11-17, later)
- [x] Coding challenges end to end: `kymo-instances::oneshot` + `DockerRuntime::run_job` (hardened one-off container; the output stream is dropped before kill or removal, or the daemon stalls), `kymo-lab::judge` (manifest `judge` block, compare modes, compile then run, a resource failure ends the run), `POST /v1/judge` on the lab host, `code.rs` and `routes/code.rs` on the platform (config, one run at a time per participant, compile errors cost no attempt, hidden tests never leave the server), challenge kind `code` (stored as `flag` plus a `challenge_code` row because the `kind` column has a CHECK), bundles carry the tests, publishing needs tests, the flag endpoint refuses a coding challenge (its placeholder flag must never solve it)
- [x] Tested for real: `crates/kymo-lab/tests/judge_docker.rs` (fork bomb, memory hog, endless loop, output flood, no network, read only filesystem, runs as nobody, no leftover container) and a live lab host with a Python and a C++ language, a real player run and a real compile error
- [x] Lab host clarity: README table "What each kind of challenge needs", Coding challenges section, `lab/judge/*/Dockerfile`, `deploy/lab/manifests/judge-*.json.example` (checked by `tests/examples.rs`), `kymo-lab check` lists languages and warns about a manifest whose image is not on the machine, the Lab hosts page lists what each challenge type needs and marks language manifests; a language manifest needs no port
- [x] Views: one `EventCard` for the player list, home, scoreboards and the organizer list; event lifecycle stepper with ordered actions; ladder with the ranking first; event leaderboard with a podium, progress bars, your row marked and the challenge grid on request; charts scale their height with their width (`chartHeight`)
- [ ] Responder challenges were not changed in this round
- [ ] Coding challenges: no local judge on the platform server (a lab host is required), no partial credit, no custom checker or interactive problems, languages are per image
- [ ] Not looked at in a browser: the coding panel for a player, the editor's Coding section, the Lab hosts page help, the C++ run in the UI (all exercised through the API and unit tests)
- [x] Monitor Live tab: charts drawn at real pixel size (taller, readable text), whole-number axis ticks (`countTicks`), drag on any chart to zoom (shared window across the charts, zoom in, out, reset, the stat tiles follow it; `cropSeries`, `scaleWindow`, `windowOf` in `lib/monitorCharts.ts`), Requests and Response time at full width, the explanatory sentences removed

## Punishing cheaters, audit (2026-11-17)
- [x] A ladder of sanctions for flag sharing: warn, then disqualify from the event (`event_participant.disqualified_at`, migration 0039: off the scoreboard, their solves no longer count for anyone's dynamic points, no more submissions, the account is untouched; a team is disqualified as a whole), then ban the account. The event's rule has a new option "Warn, then disqualify from the event" (`SharingAction::Disqualify`)
- [x] Organizer Integrity tab on a Jeopardy event: players with strikes, where, standing (Warned, Disqualified, Banned); Forgive, Disqualify (with a reason), Reinstate, Ban account. API `GET /admin/events/{id}/integrity`, `POST /admin/events/{id}/disqualify`, `DELETE .../disqualify/{user}`, `DELETE .../strikes/{user}`, all behind the moderation permission and audited
- [x] Audit tab: readable action names with a category marker (`lib/audit.ts`), targets in words; fixed a Rust debug string ("Some(1)") written as the detail of lab host changes
- [x] Copy detection for coding challenges: `kymo_jeopardy::similarity` (comments, spacing, names, numbers and strings ignored; runs of five tokens compared; programs under 40 tokens never compared), run when a solution is accepted against the latest accepted solution of every other participant (migration 0040, `code_match`); the Integrity tab lists pairs at 85% or more, with Compare (both sources side by side) and Dismiss
- [x] A disqualified player is told on the event page (`disqualified` on `GET /events/{slug}`)

## Responder review (2026-11-17)
- Reviewed and found sound: the flag is held by the lab host and released only when every check passes, definitions are read only and validated at start, players never see script output, the organizer has the Verifier log, a Responder takes two private addresses, the Verifier page is themed and accessible
- [x] Per-check `hint` in the Verifier definition (shown only while a step is failing, validated, in the schema, the template and the design doc); the Verifier page shows "N of M fixed" with a progress bar and the hint
- [x] Responder defaults to a per-player flag (a static flag is the same for everyone, so the first player to pass could share it): `applyResponder` makes it dynamic on the accepted prefix and suffix, the editor warns when it is static, and the flag box says "Paste the flag the Verifier shows"
- [x] Go-live check: a published Responder needs the event on the private VPN with a lab host, instead of failing when the first player presses Start
- [x] Replaced a static string-interning cache in the Verifier session with plain strings
- [ ] An organizer cannot try a Responder as a player from the console; `kymo-lab verifier test` against a running machine is the only way to rehearse the checks
- [ ] A hint is all or nothing; there is no progressive reveal

## Sandbox escape review (2026-11-17)
- Probed from inside real submissions and a Verifier-style container: no mount, namespace, bpf, ptrace or kernel-module syscall works; /proc/sys, /sys and cgroups are read-only; no device nodes; /tmp is noexec; no host processes, Docker socket or setuid binaries; only `lo` (judge) and no host gateway reach (Verifier network). Memory, fork, disk, fd and output floods are all contained with the right verdict and no container left behind
- [x] Judge and Verifier: private IPC (judge has none), small /dev/shm, `/tmp` now `noexec,nosuid,nodev` on the Verifier too
- [x] Live test `the_program_is_isolated_from_the_host_and_cannot_run_what_it_writes`; the leftover-container test waits for the label to drain instead of racing the other tests
- [x] Judge seccomp: Docker's default profile minus `ptrace`, `process_vm_readv`, `process_vm_writev` and the odd `personality` values (`crates/kymo-instances/src/profiles/judge-seccomp.json`); the live test proves they are denied
- [x] `KYMO_LAB_JUDGE_RUNTIME` runs submissions under gVisor `runsc` or Kata instead of sharing the host kernel (name validated; off by default; not exercised here, no such runtime installed)
- [x] Mount path: the default judge root is generic and the README says to keep it so; copying files in instead of bind mounting was rejected, because extracting a tar produced by an untrusted compiler would open a worse hole than the path leak

## Release 0.5.0 (2026-11-17)
- [x] Merged `origin/update` (dynamic and several flags in tournament matches): its migration is now `0041_duel_flags.sql`, its editor changes ported into the split editor (`draft.ts`, `FlagRows.tsx`), its duel tests pass
- [x] Coding challenges need a lab host of 0.5.0 or later; a failed run on an older one says so (`labs::judge_version_hint`)
- [x] Fixed: the console nav and the event filter widened the page on a phone; anonymous answers were served stale after a change (the public cache is cleared on every notify)
- [x] e2e specs brought up to date with the organizer console, the Infrastructure tab, the sign-in gate and the redesigned cards; all specs pass locally on Chromium

## Documentation and manifests (2026-11-17, 0.5.1)
- [x] README is now the front door: what Kymo is, a start-here table, grouped features, quickstart, running a tournament and a Jeopardy event, configuration. The long material moved to `docs/CHALLENGES.md` (coding, dynamic flags, Responder, integrity), `docs/LAB_HOST.md` (instances, lab host, VMs, manifests, lab API) and `docs/OPERATIONS.md` (monitor, deploy, Docker, backups, VPN). Every Markdown link and anchor was checked
- [x] All sample manifests carry a `_comment` saying what they are for; `vm-example` shows `disk`, `nic`, `firmware` and `display`; two new tests keep it that way (every `Manifest` field appears in an example, every example explains itself)
- [x] `deploy/lab/lab.env.example` and `deploy/kymo.env.example` list every variable (judge, name prefix, VPN directory and bind options, backups, VPN ranges); the lab host guide table matches
- [x] Release archives include the guides; messages that said "see the README" point at the right guide

## Sample manifests index (2026-11-22)
- [x] `deploy/lab/manifests/README.md`: a table of every sample manifest, what each is for and its runtime, and a note that a Windows virtual machine example is not available yet
- [ ] A Windows `.ova` sample manifest (RDP or WinRM delivery, a Windows-shaped `inject` block)

## Configurable start budget (2026-11-22)
- [x] `max_starts` (starts plus restarts a participant gets on a challenge) is now an event-wide
  default (`event.max_starts`, default 5) with a nullable per-challenge override
  (`challenge.max_starts`); previously it was the single hardcoded `Policy::default()` value (5)
  for every challenge on every event, with no way to change it short of editing code and
  redeploying
- [x] First built on `event_jeopardy.max_starts` (migration `0043_max_starts.sql`), matching the
  precedent of `rate_max_attempts`. Moved to `event.max_starts` once it became clear that gated it
  to Jeopardy events: `/admin/events/{id}/jeopardy` (`get_settings`/`put_settings`) refuses a
  tournament event server-side (409 "this is not a Jeopardy event"), so an `event_jeopardy` row,
  and the default it would carry, could never exist for one. Since 0043 was already applied to
  the dev database with that content, it was left as it was committed; `0044_max_starts_on_event.sql`
  adds `event.max_starts`, backfills it from any existing `event_jeopardy.max_starts`, and drops
  the column from `event_jeopardy`. A new `PUT /admin/events/{id}/max-starts` (`events.rs`,
  mirroring `set_network_mode`) sets it for either event kind
- [x] `instances::max_starts()` resolves the challenge's own override, else `play.event.max_starts`
  (no DB round trip: `Play` already carries the `EventRow`); `begin()` builds a per-call `Policy`
  with that value, `view()`/`current()`/`stop()` report it as `starts_max`. Stopping an instance
  still spends no budget either way
- [x] Admin UI: "Instance start limit" in the event's Infrastructure tab (shown for every event,
  not only Jeopardy); "Start limit" on a challenge's Delivery section (empty follows the event
  default)
- [x] Carried through the event bundle export/import format (`ChallengePart.max_starts`,
  `#[serde(default)]` so older bundles without it still import)
- [x] `cargo test -p kymo-server`, `cargo clippy -p kymo-server -p kymo-instances --all-targets -- -D warnings`, `tsc -b` and `npx vitest run` all pass. `the_start_budget_default_is_not_gated_to_jeopardy_events` proves a tournament event can set it while `/jeopardy` still refuses it; `the_start_budget_follows_the_events_default_unless_the_challenge_overrides_it` covers the override resolution; `the_start_budget_is_enforced_and_failed_starts_are_free` now sets its budget through a per-challenge override instead of the old fixture-level `Policy.max_starts`, which no longer drives this
- [ ] No admin action yet resets or grants a specific participant extra starts on a challenge they have exhausted; raising the event or challenge limit helps everyone, not just them
- [ ] A tournament challenge needs a scheduled match assigned to it before any player can start its instance (`duel::player_gate`, pre-existing, unrelated to this change) — not exercised end-to-end here, only the settings endpoint itself


## VPN panel collapsible (2026-11-22)

- [x] `VpnPanel` (player event page) shows only the shield icon, "Your private network" and a chevron; the steps, profile download and status open/close on click (`aria-expanded`, collapsed by default). Content stays mounted (`hidden`), so a download in progress and its messages survive a collapse. Not type-checked or run here (web deps live in the dev container): run `tsc -b` there.

## Lab installer installs Docker and QEMU (2026-11-22)

- [x] `deploy/lab/install-lab.sh` detects Debian/Ubuntu/Fedora/Arch from `/etc/os-release` (`ID`, `ID_LIKE`), installs Docker when the `docker` command is missing (`docker.io` / `moby-engine` / `docker`, enable + start, creates the `docker` group) and QEMU+OVMF+tar for VMs by default; `--no-docker-install`, `--no-vm` opt out, `--with-vm` kept as a no-op. Loads `tun` (persisted in `/etc/modules-load.d/kymo-lab.conf`); rootless Docker drops the `vm` runtime with a message
- [ ] Only `sh -n` syntax-checked: not run on a real Debian/Ubuntu/Fedora/Arch machine. Rocky/Alma (ID_LIKE contains `fedora`) would try `moby-engine`, which RHEL repos lack
- [x] Corrected: Docker now comes from Docker's own apt/dnf repository on Debian/Ubuntu/Fedora (distro package as fallback), Arch uses its `docker` package; base tools (curl, openssl, ss, iptables) installed when missing. Still untested on real machines, and SELinux (Fedora) and firewalld port opening are not handled: the installer only prints the ports to open
- [x] Installer assumes a bare host: `need CMD DEB FED ARCH` verifies/installs ~20 programs (curl, openssl, tar, ip, ss, modprobe, useradd/usermod/groupadd, runuser, kill, awk, xargs, iptables, journalctl...), CA certs, empty Arch db sync, `hostname` dependency removed, SELinux `restorecon`, firewalld/ufw port opening (closed again by --uninstall). Verified only by `sh -n` and a stubbed `need`; no real Debian/Ubuntu/Fedora/Arch run yet. Needs systemd: not installable, it dies with a message
- [x] Installer: with SELinux enforcing and the VPN on, adds `KYMO_LAB_VPN_BIND_OPTIONS=z` to lab.env (the gateway container otherwise cannot read /vpn and loops with "server.conf is missing"), restarts the service and removes the stale gateway container. Found on a real Fedora lab host; fix confirmed there only after the user applies it

## Verifier runs kept by the platform (2026-11-23)

- [x] Migration 0045 `verify_run` (instance_id, at, millis, passed, total, checks JSON; UNIQUE (instance_id, at); cascades with the instance). `verify_store.rs`: `parse_log` (skips malformed entries, unknown state reads broken, ids and details cut), `archive` (INSERT OR IGNORE, idempotent), `list`, `summaries` (runs, latest passed/total, first all-green time)
- [x] `instances.rs`: `archive_verify_log` (best effort, only for a run with `verifier_ip`, a lab host and a handle) is called in `end_run` before the container is stopped (covers stop, expiry, event end, retire, restart, admin stop), when a run is found exited, on every reaper pass for live runs, and when an organizer opens the log. `GET /admin/instances/{id}/verify-log` now reads the platform copy (404 only when there is no Verifier and nothing stored). `GET /admin/events/{id}/instances` gains `verify` per run
- [x] Web: Verify column in the admin Instances table, `passed/total` in each log entry, the log works after the instance ended
- [x] `cargo test -p kymo-server` (all 17 unit and every integration file, including the new `verifier_runs_are_kept_by_the_platform_and_read_after_the_instance_has_ended`), `cargo clippy -p kymo-server --all-targets -- -D warnings` and `tsc -b` pass. `vitest` and the other crates were not run
- [ ] Not exercised against a real lab host and Verifier: the test inserts runs through `verify_store::archive` (the lab call itself is the existing `verify_log`). Runs made before this migration are not recoverable. The event export bundle does not include them
