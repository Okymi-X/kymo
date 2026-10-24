# Tasks

## Done
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

## Done (latest)
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

## Todo
- [ ] Scoreboard import for seeding (first provider TBD), then per match result pull, see ARCHITECTURE
- [ ] Sample data seed command based on the schema
- [ ] README with quickstart
