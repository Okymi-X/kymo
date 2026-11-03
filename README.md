# Kymo

[![CI](https://github.com/Okymi-X/kymo/actions/workflows/ci.yml/badge.svg)](https://github.com/Okymi-X/kymo/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Okymi-X/kymo)](https://github.com/Okymi-X/kymo/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Open source tournament manager for CTF events. Run a knockout, double elimination, round robin, groups then knockout or Swiss tournament, show it live on a projector, and embed it on any site. A Rust API with a React interface and SQLite storage, in one small binary.

![Tournaments home with eight sample tournaments](docs/screenshots/tournaments.png)

## Contents

- [Why Kymo](#why-kymo)
- [Features](#features)
- [Screenshots](#screenshots)
- [Quickstart](#quickstart)
- [Configuration](#configuration)
- [Running an event](#running-an-event)
- [Use the API from a script](#use-the-api-from-a-script)
- [Formats](#formats)
- [Challenge instances with Docker or Podman](#challenge-instances-with-docker-or-podman)
- [Backup and restore](#backup-and-restore)
- [Private VPN for every player](#private-vpn-for-every-player)
- [Deploying on your own server](#deploying-on-your-own-server)
- [Run with Docker](#run-with-docker)
- [Development](#development)
- [Project layout](#project-layout)
- [Credits and license](#credits-and-license)

## Why Kymo

CTF events have a few recurring problems: a score that is disputed, players asking where they play next, and an organizer page that gets shared on screen by mistake. Kymo is built around those.

- Every result, correction and forfeit goes into a public activity log with its time.
- Players can follow their team and see their next match, time and room.
- The projector view is a separate page with no controls, so nothing private is ever on screen.

## Features

- **Jeopardy CTFs, solo or in teams:** categories, static or dynamic points, hints with a point cost, attachments, flag formats and regex flags, attempt limits, a scoreboard that stays hidden until you publish it, and per teammate progress. Players sign up, form teams with invite codes, register for an event and play in the browser.
- **Challenge instances:** a challenge can start a private Docker container per player or team, with CPU, memory, process and lifetime limits, an expiry reaper and cleanup of orphaned containers.
- **Roles and safety:** visitors, players, referees, monitors and admins; per endpoint rate limits with automatic and manual bans, a request log, a moderation audit trail, health and backup tools, and event bundles you can export and import.
- **Five formats:** single elimination (optional third place match), double elimination, round robin, groups then knockout, and Swiss. Any of them can be best of 1, 3, 5 or 7, entered game by game.
- **Organizer tools:** enter and correct scores, forfeits, undo the newest result, schedule matches with a place, drag teams into seeding order, seed by qualifier score or at random, decide ties by hand, rename, duplicate, back up as JSON, import a setup file.
- **A live map of the bracket:** drag it, zoom at the cursor, jump with the minimap. Every format has a Bracket tab.
- **Standings, schedule, matches, teams, activity:** all paginated with search, so a 100 team event stays readable.
- **Stage mode** at `/t/{slug}/stage`: a rotating projector view with the current round, standings and the latest results. It fits one screen with no scrolling.
- **Embeds** at `/embed/{slug}`: copy an iframe snippet from the Embed button. Only these pages may be framed.
- **Calendar feed** at `/api/v1/tournaments/{slug}/calendar.ics` for scheduled matches.
- **Appearance in the database:** site name, logo, colors for light and dark, type, corner radius, density, page tint, bracket line style and team avatars. A tournament can also carry its own accent color.
- **Live updates** over server sent events, with no page refresh.
- **Webhooks** that post results, corrections, forfeits and schedules to Discord, Slack or any URL that accepts JSON.
- **Referee tokens**: a token that can only record results and schedules for the tournaments you choose, with its own sign in.
- **Captain reporting**: each team can get a private link to submit its score, which waits for your approval.
- **Playing now**: mark a match as started, and spectators see a LIVE badge while chat gets a message.
- **Bracket image**: save the whole bracket as a PNG or SVG to post or print.
- **Late match panel**: planned matches that should have started, with one click forfeits.
- **Ladder** at `/ladder`: points across every tournament on the site.
- **Link previews** when a tournament link is pasted in chat, and an installable app on phones.
- **API tokens** with an expiry for scripts and score importers.
- **Accessible by default:** keyboard operation, visible focus, a skip link, screen reader announcements while reordering teams, and light and dark themes that follow the device.

## Screenshots

### Standings, schedule and matches

A Swiss tournament in progress. Standings rank by points, goal difference and goals scored. The organizer can decide ties by hand, and the schedule shows one round at a time.

![Swiss standings with a Decide ties button](docs/screenshots/swiss-standings.png)

![Swiss schedule showing round 3](docs/screenshots/swiss-schedule.png)

![Matches grouped into up next, waiting and played](docs/screenshots/matches.png)

### Brackets

Brackets are a map you can move around. A finished tournament shows its champion, and a live one highlights the match to play next.

![A finished single elimination bracket with a third place match](docs/screenshots/bracket-finished.png)

![A live bracket with byes, a forfeit and the final ready to play](docs/screenshots/bracket-live.png)

### Stage mode

A full screen view for a projector. It rotates between the current round, the bracket and the standings, and keeps what is next and the latest results beside it.

![Stage mode showing the current round](docs/screenshots/stage-round.png)

![Stage mode showing the standings](docs/screenshots/stage-standings.png)

### Appearance

Pick a preset or set your own colors, type, radius, logo, tint and bracket lines. Changes preview live and apply to everyone once saved.

![The appearance settings with a live preview](docs/screenshots/appearance.png)

## Quickstart

You need Rust (stable) and Node with pnpm.

```sh
git clone https://github.com/Okymi-X/kymo.git
cd kymo
(cd web && pnpm install && pnpm build)

export KYMO_ADMIN_PASSWORD='at least 12 characters'
cargo run -p kymo-server seed-demo   # optional: eight sample tournaments
KYMO_ADDR=127.0.0.1:8787 cargo run -p kymo-server
```

Open http://127.0.0.1:8787 and sign in as organizer with that password.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `KYMO_ADMIN_PASSWORD` | required | Organizer password, at least 12 characters |
| `KYMO_ADDR` | `127.0.0.1:8080` | Address to listen on |
| `KYMO_DATABASE_URL` | `sqlite://kymo.db` | SQLite database file, created on first run |
| `KYMO_TRUST_PROXY` | off | Set to `1` behind a reverse proxy so the last `X-Forwarded-For` entry is the client address (needed for correct rate limits and bans) |
| `KYMO_BACKUP_DIR` | off | Directory for scheduled database backups (created private) |
| `KYMO_BACKUP_EVERY_HOURS` | `24` | Hours between scheduled backups |
| `KYMO_BACKUP_KEEP` | `7` | Backups to keep |
| `KYMO_INSTANCES` | off | Set to `docker` to let players start challenge containers |
| `KYMO_INSTANCE_HOST` | `127.0.0.1` | Host name players are told to connect to |
| `KYMO_INSTANCE_BIND` | `0.0.0.0` | Address container ports are published on |
| `KYMO_VPN` | off | Set to `1` for a private VPN network per player (needs `KYMO_INSTANCES=docker`) |
| `KYMO_VPN_HOST` | required with the VPN | Name or address players connect to, written into their profiles |
| `KYMO_VPN_PORT` | `1194` | UDP port on the server for the VPN |
| `KYMO_VPN_BIND` | `0.0.0.0` | Address the VPN port is published on |
| `KYMO_VPN_DIR` | `kymo-vpn` | Directory shared with the gateway container (must be a path the container platform can mount) |
| `KYMO_VPN_IMAGE` | `localhost/kymo/vpn-gateway:1` | Gateway image, built from `deploy/vpn-gateway` |
| `KYMO_VPN_TUNNEL` | `10.8.0.0/16` | Address range handed to players' VPN tunnels |
| `KYMO_VPN_LABS` | `10.77.0.0/16` | Range split into one private /27 network per player or team (2048 by default) |
| `KYMO_VPN_CERT_DAYS` | `60` | How long a player's VPN certificate is valid |
| `KYMO_VPN_BIND_OPTIONS` | empty | Extra bind mount options, for example `z` on SELinux hosts |

Everything else, including the look of the site, is stored in the database and changed from the interface.

## Running an event

1. **Create** a tournament from the home page: pick a format and a name.
2. **Add teams**, one per line. Add a qualifier score after a comma to seed by score (`Alpha, 900`).
3. **Seed** by dragging, or choose manual, random or score seeding when you start.
4. **Start**, then enter results from the bracket, the schedule or the matches list. Click a match to enter a score, record a forfeit, or plan a time and place.
5. **Show it**: open Stage mode on the projector, share the tournament link with players, or embed it on your site.

If a result was wrong, correct it from the same dialog. A correction is refused when a later match already depends on it, so the bracket never ends up inconsistent. The newest result can also be undone from the Activity tab.

## Use the API from a script

Create a token on the API tokens page (choose an expiry), then:

```sh
curl -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -X PUT http://127.0.0.1:8787/api/v1/admin/tournaments/1/matches/0/result \
  -d '{"score_a": 3, "score_b": 1}'
```

Create a whole tournament from a file. The schema is `docs/schema/tournament-import.schema.json` and an example is `docs/examples/spring-cup.json`:

```sh
curl -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -X POST http://127.0.0.1:8787/api/v1/admin/import --data @docs/examples/spring-cup.json
```

Reads are public: `GET /api/v1/tournaments` and `GET /api/v1/tournaments/{slug}`. An API token cannot create other tokens.

## Formats

| Format | Teams | Notes |
| --- | --- | --- |
| Single elimination | 2 to 256 | Byes fill the first round. Optional third place match. |
| Double elimination | 3 to 256 | Winners and losers brackets, grand final and a reset match if the losers side wins. |
| Round robin | 3 to 32 | Everyone plays everyone. 3 points for a win, 1 for a draw. |
| Groups then knockout | 3 to 32 | 2 to 8 groups, 1 to 4 teams per group advance. Same group teams are kept apart in the first knockout round. |
| Swiss | 4 to 256 | 1 to 12 rounds, or log2 of the team count. Each round is paired once the last one is played, with no rematches and one bye at most per team. |

Teams level on points, difference and goals are ordered by the organizer's tiebreak list, then by seed. In a groups tournament that order also decides who qualifies.

## Challenge instances with Docker or Podman

Set `KYMO_INSTANCES=docker`, `KYMO_INSTANCE_HOST` (what players connect to) and `KYMO_INSTANCE_BIND` (where container ports are published; `127.0.0.1` for a local test). The server talks to the Docker API. With Docker, your user needs access to its socket (the `docker` group). With rootless Podman, run `systemctl --user enable --now podman.socket` and set `DOCKER_HOST=unix:///run/user/$UID/podman/podman.sock`.

An instance is shown to the player as *starting* until its port really accepts connections (checked through the gateway for a private lab); if it never does, or the container later exits, it is stopped, marked failed with a reason and does not use the player's start budget. `lab/` holds three tiny example challenge images (busybox web servers and a raw TCP service). Build them with `podman build -t localhost/kymo-lab/flag-server:1 lab/flag-server` (or `docker build`), and likewise `page-source`, then point a challenge's instance image at them. To check the runtime against a real daemon: `KYMO_TEST_IMAGE=localhost/kymo-lab/flag-server:1 cargo test -p kymo-instances --features docker --test docker`.

## Backup and restore

`GET /api/v1/admin/ops/backup/download` (organizer) returns a consistent snapshot of the whole database. To restore, stop the server and run `kymo-server restore backup.db`; the current database is kept as `<name>.pre-restore-<time>`. A single Jeopardy event can be exported and imported as a bundle with `GET /api/v1/admin/events/{id}/export` and `POST /api/v1/admin/events/import`.

## Private VPN for every player

By default an instance gets a port on the server, and anyone who can reach the server can reach it. In **VPN mode** an event's instances live on private networks instead:

```
 player's laptop                         your server
 +--------------+    UDP 1194     +---------------------------+
 | OpenVPN      |  ============>  | gateway container         |
 | (own profile)|                 |  one tunnel address each  |
 +--------------+                 |  firewall: default deny   |
                                  +--+-----------+-----------+
                                     | allowed   | not allowed
                              +------v----+  +---v-------+
                              | lab of    |  | lab of    |
                              | player A  |  | player B  |   no route out,
                              | (private) |  | (private) |   no route between labs
                              +-----------+  +-----------+
```

- Every player (or team, in team events) gets an **isolated lab network** with no route to the internet and no route to other labs. Their instances start inside it at a private address, with nothing published on the host.
- Every account gets its **own OpenVPN profile** (one file, certificate and key inside). The gateway gives each profile a fixed tunnel address and a firewall that allows that address to reach **only its own lab**. Players cannot reach each other's tunnels or labs.
- Teammates share the team's lab, each with their own profile. Leaving or being removed from a team closes the lab to that player immediately.
- Downloading a new profile revokes the old one. A ban or the admin's *Revoke* removes the player's access and drops their session. Certificates are also on a revocation list.

### Setting it up

You need a Linux host, a container platform the Kymo server can talk to (rootful Docker, or Podman through its Docker compatible socket), the `/dev/net/tun` device, and one UDP port reachable from your players (default 1194).

1. **Build the gateway image** (a small Alpine image with OpenVPN and iptables; it holds no secrets):

   ```sh
   docker build -t localhost/kymo/vpn-gateway:1 deploy/vpn-gateway     # or: podman build ...
   ```

2. **Turn it on** in the server environment, next to the instance settings:

   ```sh
   KYMO_INSTANCES=docker
   KYMO_VPN=1
   KYMO_VPN_HOST=vpn.example.org      # what players connect to; a DNS name or an IPv4 address
   KYMO_VPN_DIR=/var/lib/kymo/vpn     # an absolute path on the host
   ```

   On first start Kymo creates its own certificate authority, writes the gateway's config and certificates into `KYMO_VPN_DIR`, starts the container `kymo-vpn-gateway`, and logs `vpn ready`. If the gateway cannot start (image missing, platform down) the server still starts, the *Operations* page shows what is wrong, and the maintenance task retries every 10 minutes.

3. **Open the UDP port** in your firewall (`ufw allow 1194/udp`). Only this port needs to be reachable.

4. **Switch an event to VPN mode:** *Manage events*, open the event, **Overview**, **Instance network**, choose *Private VPN*. Do this while no instance is running.

5. **Players** open the event page, follow *Your private network* (install OpenVPN 2.5 or newer, download the profile, connect), then start their instances. The challenge shows the private address, for example `10.77.0.10:8000`.

6. **Check it** on *Operations*: the gateway should say *Running*, with the private networks and players it knows. *Sync the gateway now* repairs any drift. The same page can revoke a player's VPN by user id.

### Things to know

- **Run Kymo on the host** (the systemd unit in `deploy/`), not inside a container, when you use the VPN. The gateway's directory is mounted by the container platform, which resolves the path on the host.
- **Do not run other containers on the networks Kymo creates**, and keep untrusted containers off the host's default network. Isolation between labs relies on them being separate networks; Kymo attaches only the gateway to them. Challenge containers run with no capabilities, no new privileges and CPU, memory and process limits.
- **The certificate authority's private key is stored in the database** (setting `vpn_ca`), so a database backup restores every issued profile. Treat backups as secrets. Losing the database means players download new profiles.
- **Unused labs are removed** 10 minutes after their last instance stops, and as soon as the event ends. They are created again on demand.
- **Limits:** by default 2048 private networks (one /27 each, 21 instances per network) and about 65,000 players. Change the ranges with `KYMO_VPN_TUNNEL` and `KYMO_VPN_LABS` before the first start. How many labs a single gateway handles comfortably has not been measured here; each lab adds one network interface to the gateway, so very large events may want several servers.
- **Rootless Podman** is fine for trying it out. The gateway disables OpenVPN's kernel fast path (DCO), which rootless containers cannot use; player profiles do not need to, because players run OpenVPN on their own machines. On rootless Podman, containers on the host's default network can still route to lab addresses, so use rootful Docker or a dedicated host for real events.
- **Verify a setup yourself** with the integration test, which starts a gateway, two labs and two VPN clients and checks reach, isolation and revocation:

  ```sh
  KYMO_TEST_CLI=docker \
  KYMO_TEST_GATEWAY_IMAGE=localhost/kymo/vpn-gateway:1 \
  KYMO_TEST_IMAGE=localhost/kymo-lab/flag-server:1 \
  cargo test -p kymo-vpn --features docker --test gateway -- --nocapture
  ```

## Deploying on your own server

A complete setup is one binary, one SQLite file and, optionally, a container platform for instances. The files in `deploy/` are starting points.

1. **Build.** You need Rust (stable), Node 22 and pnpm. The web interface is embedded in the binary, so build it first:

   ```sh
   cd web && pnpm install --frozen-lockfile && pnpm build && cd ..
   cargo build --release -p kymo-server        # target/release/kymo-server
   ```

   Or build the container image with `docker build -t kymo .` (it does both steps).

2. **Configure.** Copy `deploy/kymo.env.example` to `/etc/kymo/kymo.env` and edit it. At least set `KYMO_ADMIN_PASSWORD`. Keep the file readable only by root and the service user.

3. **Run as a service.** `deploy/kymo.service` is a hardened systemd unit (dedicated user, read-only system, writable state directory only). The install commands are in its header.

4. **Put HTTPS in front.** Run Kymo on `127.0.0.1:8080` and use a reverse proxy: `deploy/Caddyfile.example` (automatic certificates) or `deploy/nginx.conf.example`. Set `KYMO_TRUST_PROXY=1` so rate limits and bans use the real client address, and make sure the proxy does not buffer `text/event-stream` responses (live updates). Without `KYMO_TRUST_PROXY` every player would share the proxy's address and one automatic ban could lock everyone out.

5. **Instances and the VPN (optional).** Add the service user to the `docker` group, or use Podman as described above. **Membership of the docker group is equivalent to root** on that machine, so run a dedicated server for the event, and do not expose the Docker socket to Kymo in a container on a shared host.

6. **First sign in.** Open `/login` with the organizer password, set the look of the site under *Appearance*, then *Manage events*. Players create their own accounts; you can close sign-ups under *Users*.

7. **Back up.** Set `KYMO_BACKUP_DIR` for scheduled snapshots and copy them off the machine; the *Operations* page also downloads one on demand. To restore, stop the service and run `kymo-server restore backup.db`.

8. **Update.** Take a backup, install the new binary, restart. Database migrations run on start. A backup from an older version restores into a newer binary; the reverse is refused.

**Security checklist before the event:** a long organizer password; HTTPS in front; `KYMO_TRUST_PROXY` matching your proxy; the firewall allows only 443 (and the VPN UDP port or the instance port range if you use them); backups copied off the server and treated as secrets; the container platform socket reachable only by the service user; a test event played end to end with a spare account.

## Run with Docker

```sh
KYMO_ADMIN_PASSWORD='at least 12 characters' docker compose up --build
```

The app serves the interface and the API on port 8080. Data lives in the `kymo-data` volume. This image is for tournaments and plain events; challenge instances and the VPN need the container platform's socket and host paths, so run the binary on the host for those (see above).

## Development

```sh
export KYMO_ADMIN_PASSWORD='at least 12 characters'
KYMO_ADDR=127.0.0.1:8787 cargo run -p kymo-server
cd web && pnpm install && KYMO_API=http://127.0.0.1:8787 pnpm dev
```

Run the checks:

```sh
cargo test && cargo clippy --all-targets -- -D warnings
cd web && pnpm exec tsc -b && pnpm exec vitest run
```

End to end tests start their own server on port 8791 with a throwaway database and drive the organizer flow in Google Chrome. Build the web app first.

```sh
cd web && pnpm build && pnpm e2e
```

Notes for contributors and coding agents live in `docs/`: `RULES.md`, `ARCHITECTURE.md`, `DESIGN.md`, `TASKS.md` and `MEMORY.md`.

## Project layout

```
crates/kymo-engine     pure bracket logic: generation, results, standings, Swiss pairing
crates/kymo-core       pure event rules: lifecycle states, schedule windows, roles, team rosters
crates/kymo-jeopardy   pure Jeopardy rules: flags, static and dynamic scoring, ranking, rate limits
crates/kymo-instances  challenge container lifecycle: policy, runtime interface, Docker and Podman
crates/kymo-monitor    pure request rules: per endpoint limits, automatic bans, ban expiry
crates/kymo-vpn        per player OpenVPN: certificate authority, profiles, firewall rules, gateway
crates/kymo-server     Axum API, SQLite storage, live updates, serves the built web app
web/                   React, TypeScript, Vite and Tailwind interface, with Playwright tests
deploy/                systemd unit, reverse proxy examples, the VPN gateway image
lab/                   small example challenge images used by the tests and the demo lab
docs/                  product, architecture, design notes, import schema and screenshots
```

## Credits and license

Team avatars are generated with [blobatar](https://github.com/Alain00/blobatar) (MIT). The same team name always gets the same avatar.

Released under the MIT license, see `LICENSE`.

## Releases

Releases are automatic. When `version` in `Cargo.toml` has no matching tag, the next push to `main` builds the web app and the server on Ubuntu 22.04 and publishes a GitHub release `vX.Y.Z` with a Linux x86_64 tarball (the server with the web app embedded, plus `deploy/`) and a SHA-256 checksum. To release: bump the version, commit, push. Pushing a `v*` tag by hand or running the workflow from the Actions tab works too. Check a download with `sha256sum -c kymo-vX.Y.Z-linux-x86_64.tar.gz.sha256`.
