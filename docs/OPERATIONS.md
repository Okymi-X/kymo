# Operations guide

Running Kymo for real: the monitor and alerts, deploying on your own server or with Docker, backups, and the private VPN that gives every player their own path to the challenge machines.

[Back to the README](../README.md)

## Monitor: live graphs and attack alerts

**Live tab** (the default). Requests per bucket stacked by outcome (succeeded, refused or not found, rate limited, server error), what people do (sign ins, failed sign ins, new accounts, flag submissions, instances started), players and addresses online, response time, the busiest players and addresses of the last 5 minutes with a trend line, and a week heat map. The range is 15 minutes, 1 hour, 1 day or 1 week, and the view refreshes every 5 seconds. A bucket far above the rest (over three standard deviations above the mean, at least twice the mean and at least 10 requests) is marked as a peak. Click a bar or point to open the Traffic tab on the requests of that moment; red triangles mark alerts and open them.

**Alerts tab.** A check every 30 seconds runs six detectors over the recent request log. Each counts one kind of behaviour per signed in player (or per address when nobody is signed in) inside its own time window:

| Detector | Counts | Alert at | Auto-ban at |
| --- | --- | --- | --- |
| Path scanning | different unknown or missing addresses | 25 in 2 min | 120 |
| Admin probing | refusals on admin routes (a visitor counts each different address once) | 5 in 5 min | 25 |
| Attack strings | requests with `../`, injection or script strings, or probes for `.env` and `.git` | 2 in 5 min | 10 |
| Scanner tool | user agents such as sqlmap, nikto, gobuster, ffuf, nmap | 1 in 5 min | 20 |
| Failed sign ins | failed sign ins from one address | 8 in 5 min | 40 |
| Request flood | all requests | 600 in 1 min | 2000 |

Reaching the alert level raises an alert (high when the ban level was reached, medium for a signed in player below it, low for a visitor). Reaching the ban level bans a **signed in player** automatically, by default for 15 minutes. An address is never banned automatically, because it can be a whole network; you can ban it by hand. Staff are never counted. Every number is editable under Detection settings (alert level, ban level or never, window, ban length from 5 minutes to permanent), per detector, with a restore button. An alert shows the examples, a per minute chart around it and the requests behind it, and you can ban the player for 5 minutes, 15 minutes, 1 hour, 1 day or permanently, ban the address, or dismiss it with a note. Behaviour you dismissed is not raised again; new behaviour after that is. The Monitor entry in the account menu shows a badge with the open alerts about players.

**What is stored.** The request log now also keeps the query string (up to 300 characters) and the user agent (up to 200), so the detectors can see attack strings and scanner tools. Request bodies, passwords and session tokens are never stored. Keep secrets out of URLs. Rows are deleted after 7 days.

**Temporary bans of players now end.** A ban with a length used to set the account's banned flag for good; a sweep (every 30 seconds) now ends the ban and lifts the flag when the time is up, unless another ban on the same player still applies.

**API.** `GET /admin/monitor/series?minutes=`, `/hot`, `/heatmap`, `/alerts?status=&who=&page=`, `/alerts/{id}`, `POST /alerts/{id}/dismiss`, `POST /alerts/{id}/ban`, `GET /detectors`, `PUT` and `DELETE /detectors/{kind}`. Reading needs the monitor permission; banning, dismissing and changing detectors need the moderation one. The VPN side (players scanning the lab host from inside the VPN) is not covered yet.

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

The app serves the interface and the API on port 8080. Data lives in the `kymo-data` volume. The image checks its own health every 30 seconds (`GET /health`), so `docker compose ps` shows `healthy` or `unhealthy` and the `unless-stopped` restart policy can be paired with a watcher if you want automatic restarts. This image is for tournaments and plain events; challenge instances and the VPN need the container platform's socket and host paths, so run the binary on the host for those (see above).

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
