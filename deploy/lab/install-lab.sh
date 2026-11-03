#!/bin/sh
# Installs the Kymo lab host on this machine: a service that runs challenge containers for a Kymo
# platform. Run it as root from the unpacked release. Safe to run again to upgrade.
#
#   sudo ./install-lab.sh [--public-host ADDRESS] [--port 7400] [--allow-from CIDR] [--mode manifest|open]
#   sudo ./install-lab.sh --uninstall [--purge]
set -eu

PORT=7400
MODE=manifest
PUBLIC_HOST=""
ALLOW_FROM=""
UNINSTALL=0
PURGE=0
HERE=$(cd "$(dirname "$0")" && pwd)

die() { echo "install-lab: $*" >&2; exit 1; }
say() { echo "==> $*"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --public-host) PUBLIC_HOST=${2:?--public-host needs a value}; shift 2 ;;
        --port) PORT=${2:?--port needs a value}; shift 2 ;;
        --allow-from) ALLOW_FROM=${2:?--allow-from needs a value}; shift 2 ;;
        --mode) MODE=${2:?--mode needs a value}; shift 2 ;;
        --uninstall) UNINSTALL=1; shift ;;
        --purge) PURGE=1; shift ;;
        -h|--help) sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown option $1 (try --help)" ;;
    esac
done

[ "$(id -u)" -eq 0 ] || die "run as root (sudo ./install-lab.sh)"
[ "$MODE" = manifest ] || [ "$MODE" = open ] || die "--mode is manifest or open"

if [ "$UNINSTALL" -eq 1 ]; then
    say "Stopping and removing the service"
    systemctl disable --now kymo-lab 2>/dev/null || true
    rm -f /etc/systemd/system/kymo-lab.service /usr/local/bin/kymo-lab
    systemctl daemon-reload
    if [ "$PURGE" -eq 1 ]; then
        rm -rf /etc/kymo-lab /var/lib/kymo-lab
        userdel kymo-lab 2>/dev/null || true
        say "Removed settings, certificate and state"
    else
        say "Kept /etc/kymo-lab and /var/lib/kymo-lab (use --purge to remove them)"
    fi
    say "Containers started by the lab host are not removed: docker ps --filter label=kymo.managed=1"
    exit 0
fi

# The binary sits next to this script in the lab release, or two levels up in the full release.
BIN=""
for candidate in "$HERE/kymo-lab" "$HERE/../../kymo-lab" "./kymo-lab"; do
    [ -x "$candidate" ] && { BIN=$candidate; break; }
done
[ -n "$BIN" ] || die "kymo-lab binary not found next to this script"

command -v docker >/dev/null 2>&1 || die "Docker is not installed. Install it first (https://docs.docker.com/engine/install/) and run this again."
docker info >/dev/null 2>&1 || die "Docker is installed but not running or not reachable by root: systemctl start docker"
command -v openssl >/dev/null 2>&1 || die "openssl is needed to make the token"
command -v systemctl >/dev/null 2>&1 || die "this installer needs systemd"

if [ -z "$PUBLIC_HOST" ]; then
    PUBLIC_HOST=$(curl -fsS -m 5 https://api.ipify.org 2>/dev/null || true)
    [ -n "$PUBLIC_HOST" ] || PUBLIC_HOST=$(hostname -I 2>/dev/null | awk '{print $1}')
    [ -n "$PUBLIC_HOST" ] || die "could not work out this machine's address: pass --public-host"
    say "Using $PUBLIC_HOST as this machine's address (change it with --public-host)"
fi

say "Creating the service user"
id kymo-lab >/dev/null 2>&1 || useradd --system --home /var/lib/kymo-lab --shell /usr/sbin/nologin kymo-lab
getent group docker >/dev/null 2>&1 || die "there is no docker group"
usermod -aG docker kymo-lab

say "Installing the program"
install -m 0755 "$BIN" /usr/local/bin/kymo-lab
install -d -m 0750 -o root -g kymo-lab /etc/kymo-lab
install -d -m 0755 -o root -g kymo-lab /etc/kymo-lab/manifests
install -d -m 0750 -o kymo-lab -g kymo-lab /var/lib/kymo-lab
# Example manifests, never active until renamed to .json.
for example in "$HERE"/manifests/*.json.example; do
    [ -f "$example" ] || continue
    [ -f "/etc/kymo-lab/manifests/$(basename "$example")" ] || install -m 0644 "$example" /etc/kymo-lab/manifests/
done

if [ -f /etc/kymo-lab/lab.env ]; then
    say "Keeping the existing settings and token in /etc/kymo-lab/lab.env"
else
    say "Writing /etc/kymo-lab/lab.env with a new token"
    TOKEN=$(openssl rand -hex 24)
    {
        echo "KYMO_LAB_TOKEN=$TOKEN"
        echo "KYMO_LAB_ADDR=0.0.0.0:$PORT"
        echo "KYMO_LAB_NAME=$(hostname)"
        echo "KYMO_LAB_MODE=$MODE"
        echo "KYMO_LAB_MAX_INSTANCES=100"
        echo "KYMO_LAB_TLS_NAMES=$PUBLIC_HOST"
        [ -z "$ALLOW_FROM" ] || echo "KYMO_LAB_ALLOW_FROM=$ALLOW_FROM"
        echo "KYMO_LAB_DATA=/var/lib/kymo-lab"
        echo "KYMO_LAB_MANIFESTS=/etc/kymo-lab/manifests"
    } > /etc/kymo-lab/lab.env
    chown root:kymo-lab /etc/kymo-lab/lab.env
    chmod 0640 /etc/kymo-lab/lab.env
fi

say "Installing the service"
if [ -f "$HERE/kymo-lab.service" ]; then
    install -m 0644 "$HERE/kymo-lab.service" /etc/systemd/system/kymo-lab.service
else
    die "kymo-lab.service not found next to this script"
fi
systemctl daemon-reload
systemctl enable kymo-lab >/dev/null 2>&1
systemctl restart kymo-lab

say "Waiting for it to answer"
ADDR=$(grep '^KYMO_LAB_ADDR=' /etc/kymo-lab/lab.env | cut -d= -f2)
LPORT=${ADDR##*:}
ok=0
for _ in 1 2 3 4 5 6 7 8 9 10; do
    if curl -fsSk -m 3 "https://127.0.0.1:$LPORT/healthz" >/dev/null 2>&1 || curl -fsS -m 3 "http://127.0.0.1:$LPORT/healthz" >/dev/null 2>&1; then ok=1; break; fi
    sleep 1
done
[ "$ok" -eq 1 ] || { journalctl -u kymo-lab -n 20 --no-pager || true; die "the service did not come up (see above)"; }

# The service made its certificate when it started; this reads the fingerprint from it.
FINGERPRINT=$(runuser -u kymo-lab -- env $(grep -v '^#' /etc/kymo-lab/lab.env | xargs) /usr/local/bin/kymo-lab fingerprint 2>/dev/null || true)
TOKEN=$(grep '^KYMO_LAB_TOKEN=' /etc/kymo-lab/lab.env | cut -d= -f2)
SCHEME=https
grep -q '^KYMO_LAB_TLS=off' /etc/kymo-lab/lab.env && SCHEME=http

cat <<DONE

Kymo lab host is running.

Add it in the platform (Lab hosts, Add a lab host) with:

  API address:             $SCHEME://$PUBLIC_HOST:$LPORT
  Token:                   $TOKEN
  Certificate fingerprint: ${FINGERPRINT:-(plain http, none)}
  Public address:          $PUBLIC_HOST

Before players use it:
  1. Open TCP $LPORT to the platform only$( [ -n "$ALLOW_FROM" ] && echo " (the API also only answers $ALLOW_FROM)").
  2. Open the instance ports to players: Docker publishes each instance on a high port, by default
     32768-60999 (cat /proc/sys/net/ipv4/ip_local_port_range).
  3. Put the challenge images on this machine (docker pull, or docker build) and describe each one
     in /etc/kymo-lab/manifests/NAME.json (examples ending in .json.example are there), then:
     kymo-lab check

Logs: journalctl -u kymo-lab -f      Upgrade: run this script again from a newer release.
The token is in /etc/kymo-lab/lab.env, readable only by root and the service.
DONE
