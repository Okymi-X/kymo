# Tasks

## Done
- [x] Server: login throttle (5 failures per minute shared budget, 429), test added
- [x] Web: bracket layout with SVG connectors and zoom (buttons, ctrl+wheel), layout unit tests (4)
- [x] Web embedded in the server binary (rust-embed), security headers, Dockerfile and compose (image builds and serves UI and API)
- [x] Web: tokens, shell, home, login, live bracket view (SSE), score dialog, drag and drop roster, start/reset/delete. Builds clean, bracket rendering checked in a browser screenshot
- [x] Server: SQLite, tournament/team/match endpoints, admin session auth, SSE (3 integration tests)
- [x] Docs initialized
- [x] Workspace scaffold, engine crate: single and double elimination, byes, edit, undo (11 tests, clippy clean)

## In progress

## Todo
- [ ] Engine: seeding strategies (random with seed, by score)
- [ ] Web: command palette, cross-section links into the grand final
- [ ] Web: component and e2e tests
- [ ] Server: API tokens, persistent sessions
- [ ] README with quickstart
