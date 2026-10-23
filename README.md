# Kymo

Open source tournament manager for CTF events. A Rust API with a React interface and SQLite storage, built for brackets that are easy to run, show on a projector and embed on another site.

## What it does

- **Formats:** single elimination (optional third place match), double elimination, round robin, groups then knockout, and Swiss.
- **Organizer tools:** score entry and correction, forfeits, match scheduling with a place, drag to seed, qualifier scores, tiebreak order, rename, JSON backup, an activity log.
- **For everyone else:** a live bracket you can drag and zoom like a map, standings, schedules, a calendar feed (`/api/v1/tournaments/{slug}/calendar.ics`), follow a team, search with Ctrl K.
- **Stage mode** (`/t/{slug}/stage`): a rotating projector view with the current round, standings and the latest results.
- **Embeds** (`/embed/{slug}`): an iframe snippet for any site, built from the Embed button.
- **Appearance:** site name, logo, colors for light and dark, type, corner radius, density, page tint, bracket line style. Stored in the database and applied before the first paint.
- **API tokens** with an expiry for scripts and score importers.

## Quickstart

```sh
export KYMO_ADMIN_PASSWORD='at least 12 characters'
cargo run -p kymo-server seed-demo   # optional: eight sample tournaments
KYMO_ADDR=127.0.0.1:8787 cargo run -p kymo-server
```

Open http://127.0.0.1:8787 and sign in as organizer with that password. Build the web app once so the server can serve it (`cd web && pnpm install && pnpm build`), or run `pnpm dev` against the API.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `KYMO_ADMIN_PASSWORD` | required | Organizer password, at least 12 characters |
| `KYMO_ADDR` | `127.0.0.1:8080` | Address to listen on |
| `KYMO_DATABASE_URL` | `sqlite://kymo.db` | SQLite database file |

## Use the API from a script

Create a token on the API tokens page, then:

```sh
curl -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -X PUT http://127.0.0.1:8787/api/v1/admin/tournaments/1/matches/0/result \
  -d '{"score_a": 3, "score_b": 1}'
```

Create a whole tournament from a file (see `docs/schema/tournament-import.schema.json` and `docs/examples/spring-cup.json`):

```sh
curl -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -X POST http://127.0.0.1:8787/api/v1/admin/import --data @docs/examples/spring-cup.json
```

Reads are public: `GET /api/v1/tournaments` and `GET /api/v1/tournaments/{slug}`.

## Run in development

```sh
export KYMO_ADMIN_PASSWORD='at least 12 characters'
KYMO_ADDR=127.0.0.1:8787 cargo run -p kymo-server
cd web && pnpm install && KYMO_API=http://127.0.0.1:8787 pnpm dev
```

Open the Vite URL, sign in as organizer, create a tournament, add teams, start.

## Run with Docker

```sh
KYMO_ADMIN_PASSWORD='at least 12 characters' docker compose up --build
```

The app serves the UI and API on port 8080. Data lives in the `kymo-data` volume.

## Test

```sh
cargo test && cargo clippy --all-targets -- -D warnings
cd web && pnpm exec tsc -b
```

Docs for contributors and agents live in `docs/`.

## End to end tests

```sh
cd web && pnpm e2e
```

Starts its own server on port 8791 with a throwaway database and drives the organizer flow in Google Chrome. Build the web app first (`pnpm build`).

## Credits

Team avatars are generated with [blobatar](https://github.com/Alain00/blobatar) (MIT). The same team name always gets the same avatar.
