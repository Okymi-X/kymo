#!/bin/sh
# Installs the Kymo lab host on this machine: a service that runs challenge containers for a Kymo
# platform. Run it as root from the unpacked release. Safe to run again to upgrade.
#
#   sudo ./install-lab.sh [--public-host ADDRESS] [--port 7400] [--allow-from CIDR] [--mode manifest|open]
#                         [--with-vm] [--runtimes docker,vm] [--vpn-host ADDRESS] [--vpn-port PORT] [--no-vpn]
#   sudo ./install-lab.sh --uninstall [--purge]
#
# Containers need Docker. Virtual machines need QEMU: --with-vm installs it (Debian, Ubuntu, Fedora,
# Arch) when it is missing. With neither installed there is nothing to run.
#
# By default players reach instances through a VPN gateway that runs on this machine (needs Docker
# for the gateway container): no instance is published on any port, only the gateway's UDP port
# faces players. --vpn-host is the address players connect to (default: the public address).
# --no-vpn publishes each instance on its own port instead.
set -eu

PORT=7400
MODE=manifest
PUBLIC_HOST=""
ALLOW_FROM=""
UNINSTALL=0
PURGE=0
WITH_VM=0
RUNTIMES=""
VPN=1
VPN_HOST=""
VPN_PORT=1194
VPN_PORT_EXPLICIT=0
VPN_POOL=10.77.0.0/16
HERE=$(cd "$(dirname "$0")" && pwd)

die() { echo "install-lab: $*" >&2; exit 1; }
say() { echo "==> $*"; }

udp_port_in_use() {
    if command -v ss >/dev/null 2>&1; then
        [ -n "$(ss -H -lun "sport = :$1" 2>/dev/null)" ]
    elif command -v netstat >/dev/null 2>&1; then
        netstat -lun 2>/dev/null | awk 'NR > 2 { print $4 }' | grep -Eq "(^|:)$1$"
    else
        return 1
    fi
}

lab_gateway_owns_port() {
    command -v docker >/dev/null 2>&1 || return 1
    docker port kymo-lab-vpn-gateway 1194/udp 2>/dev/null | grep -Eq ":$1$"
}

choose_vpn_port() {
    case "$VPN_PORT" in
        ''|*[!0-9]*) die "--vpn-port must be a number from 1 to 65535" ;;
    esac
    [ "$VPN_PORT" -ge 1 ] && [ "$VPN_PORT" -le 65535 ] || die "--vpn-port must be a number from 1 to 65535"
    if ! udp_port_in_use "$VPN_PORT" || lab_gateway_owns_port "$VPN_PORT"; then
        return
    fi
    if [ "$VPN_PORT_EXPLICIT" -eq 1 ]; then
        die "UDP $VPN_PORT is already in use on this machine; choose another --vpn-port"
    fi
    candidate=$((VPN_PORT + 1))
    while [ "$candidate" -le 65535 ] && udp_port_in_use "$candidate"; do
        candidate=$((candidate + 1))
    done
    [ "$candidate" -le 65535 ] || die "could not find a free UDP port for the VPN gateway"
    say "UDP $VPN_PORT is already in use; using $candidate for this lab host's VPN gateway"
    VPN_PORT=$candidate
}

save_existing_vpn_port() {
    env_file=/etc/kymo-lab/lab.env
    temp=$(mktemp /etc/kymo-lab/lab.env.XXXXXX)
    if grep -q '^KYMO_LAB_VPN_PORT=' "$env_file"; then
        sed "s/^KYMO_LAB_VPN_PORT=.*/KYMO_LAB_VPN_PORT=$VPN_PORT/" "$env_file" > "$temp"
    else
        cp "$env_file" "$temp"
        printf 'KYMO_LAB_VPN_PORT=%s\n' "$VPN_PORT" >> "$temp"
    fi
    chown root:kymo-lab "$temp"
    chmod 0640 "$temp"
    mv "$temp" "$env_file"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --public-host) PUBLIC_HOST=${2:?--public-host needs a value}; shift 2 ;;
        --port) PORT=${2:?--port needs a value}; shift 2 ;;
        --allow-from) ALLOW_FROM=${2:?--allow-from needs a value}; shift 2 ;;
        --mode) MODE=${2:?--mode needs a value}; shift 2 ;;
        --with-vm) WITH_VM=1; shift ;;
        --runtimes) RUNTIMES=${2:?--runtimes needs a value}; shift 2 ;;
        --vpn-host) VPN_HOST=${2:?--vpn-host needs a value}; shift 2 ;;
        --vpn-port) VPN_PORT=${2:?--vpn-port needs a value}; VPN_PORT_EXPLICIT=1; shift 2 ;;
        --no-vpn) VPN=0; shift ;;
        --uninstall) UNINSTALL=1; shift ;;
        --purge) PURGE=1; shift ;;
        -h|--help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown option $1 (try --help)" ;;
    esac
done

[ "$(id -u)" -eq 0 ] || die "run as root (sudo ./install-lab.sh)"
[ "$MODE" = manifest ] || [ "$MODE" = open ] || die "--mode is manifest or open"

if [ "$UNINSTALL" -eq 1 ]; then
    say "Stopping and removing the service"
    systemctl disable --now kymo-lab 2>/dev/null || true
    systemctl disable --now kymo-lab-firewall 2>/dev/null || true
    rm -f /etc/systemd/system/kymo-lab.service /usr/local/bin/kymo-lab
    rm -f /etc/systemd/system/kymo-lab-firewall.service /usr/local/sbin/kymo-lab-firewall
    if command -v iptables >/dev/null 2>&1; then
        iptables -w -D INPUT -s "${VPN_POOL:-10.77.0.0/16}" -j KYMO-LAB-IN 2>/dev/null || true
        iptables -w -F KYMO-LAB-IN 2>/dev/null || true
        iptables -w -X KYMO-LAB-IN 2>/dev/null || true
    fi
    systemctl daemon-reload
    command -v docker >/dev/null 2>&1 && docker rm -f kymo-lab-vpn-gateway >/dev/null 2>&1 || true
    if command -v ufw >/dev/null 2>&1; then
        # An earlier version added this rule.
        ufw delete allow proto tcp from 10.77.0.0/16 to 10.77.0.0/16 port 40000:40999 >/dev/null 2>&1 || true
    fi
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

# What this machine can run.
if [ "$WITH_VM" -eq 1 ] && ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    say "Installing QEMU"
    if command -v apt-get >/dev/null 2>&1; then
        DEBIAN_FRONTEND=noninteractive apt-get install -y qemu-system-x86 qemu-utils ovmf tar
    elif command -v dnf >/dev/null 2>&1; then
        dnf install -y qemu-kvm qemu-img edk2-ovmf tar
    elif command -v pacman >/dev/null 2>&1; then
        pacman -S --needed --noconfirm qemu-base edk2-ovmf tar
    else
        die "install QEMU yourself (qemu-system-x86_64 and qemu-img), then run this again"
    fi
fi
HAS_DOCKER=0; HAS_QEMU=0
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then HAS_DOCKER=1; fi
if command -v qemu-system-x86_64 >/dev/null 2>&1 && command -v qemu-img >/dev/null 2>&1; then HAS_QEMU=1; fi
if [ "$HAS_DOCKER" -eq 0 ] && [ "$HAS_QEMU" -eq 0 ]; then
    die "this machine has nothing to run challenges with. Install Docker (https://docs.docker.com/engine/install/) for containers, or run this with --with-vm for virtual machines."
fi
if [ -z "$RUNTIMES" ]; then
    RUNTIMES=""
    [ "$HAS_DOCKER" -eq 1 ] && RUNTIMES="docker"
    [ "$HAS_QEMU" -eq 1 ] && RUNTIMES="${RUNTIMES:+$RUNTIMES,}vm"
fi
case ",$RUNTIMES," in *,docker,*) [ "$HAS_DOCKER" -eq 1 ] || die "--runtimes asks for docker, which is not installed or not running" ;; esac
case ",$RUNTIMES," in *,vm,*) [ "$HAS_QEMU" -eq 1 ] || die "--runtimes asks for vm, but QEMU is not installed (run with --with-vm)" ;; esac
say "Running: $RUNTIMES"
if [ "$VPN" -eq 1 ]; then
    case ",$RUNTIMES," in
        *,docker,*) ;;
        *) say "The VPN gateway needs Docker, which this lab host is not set to use: continuing without the VPN (instances will be published on ports)"; VPN=0 ;;
    esac
fi
command -v openssl >/dev/null 2>&1 || die "openssl is needed to make the token"
command -v systemctl >/dev/null 2>&1 || die "this installer needs systemd"

if [ -z "$PUBLIC_HOST" ]; then
    PUBLIC_HOST=$(curl -fsS -m 5 https://api.ipify.org 2>/dev/null || true)
    [ -n "$PUBLIC_HOST" ] || PUBLIC_HOST=$(hostname -I 2>/dev/null | awk '{print $1}')
    [ -n "$PUBLIC_HOST" ] || die "could not work out this machine's address: pass --public-host"
    say "Using $PUBLIC_HOST as this machine's address (change it with --public-host)"
fi

if [ "$VPN" -eq 1 ]; then
    [ -n "$VPN_HOST" ] || VPN_HOST=$PUBLIC_HOST
    [ -e /dev/net/tun ] || die "the VPN gateway needs /dev/net/tun (modprobe tun), or run with --no-vpn"
    GW_DIR=""
    for candidate in "$HERE/vpn-gateway" "$HERE/../vpn-gateway"; do
        [ -f "$candidate/Dockerfile" ] && { GW_DIR=$candidate; break; }
    done
    [ -n "$GW_DIR" ] || die "the vpn-gateway directory is not next to this script"
    say "Building the VPN gateway image"
    docker build -q -t localhost/kymo/vpn-gateway:1 "$GW_DIR" >/dev/null || die "could not build the gateway image (it needs to download Alpine packages)"
fi

# The Verifier image, for Responder challenges: one container per instance, started by the lab
# host. It is built when the release carries it (the binary and helpers next to a Dockerfile).
VER_DIR=""
for candidate in "$HERE/verifier" "$HERE/../verifier"; do
    [ -f "$candidate/Dockerfile" ] && [ -f "$candidate/kymo-verifier" ] && { VER_DIR=$candidate; break; }
done
if [ -n "$VER_DIR" ] && [ "$HAS_DOCKER" -eq 1 ]; then
    say "Building the Verifier image"
    docker build -q -t localhost/kymo/verifier:1 "$VER_DIR" >/dev/null || say "could not build the Verifier image: Responder challenges will not start here (it needs to download Debian packages)"
fi

say "Creating the service user"
id kymo-lab >/dev/null 2>&1 || useradd --system --home /var/lib/kymo-lab --shell /usr/sbin/nologin kymo-lab
GROUPS_LINE=""
case ",$RUNTIMES," in *,docker,*)
    getent group docker >/dev/null 2>&1 || die "there is no docker group"
    usermod -aG docker kymo-lab
    GROUPS_LINE="docker" ;;
esac
case ",$RUNTIMES," in *,vm,*)
    # Hardware acceleration: without the kvm group machines still run, many times slower.
    if getent group kvm >/dev/null 2>&1 && [ -e /dev/kvm ]; then
        usermod -aG kvm kymo-lab
        GROUPS_LINE="${GROUPS_LINE:+$GROUPS_LINE }kvm"
    else
        say "No /dev/kvm here: virtual machines will run in slow emulation. Enable virtualization (and nested virtualization on a cloud server) for KVM."
    fi ;;
esac

say "Installing the program"
install -m 0755 "$BIN" /usr/local/bin/kymo-lab
install -d -m 0750 -o root -g kymo-lab /etc/kymo-lab
# Manifests may carry an SSH password for writing flags, so they are not readable by everyone.
install -d -m 0750 -o root -g kymo-lab /etc/kymo-lab/manifests
chmod 0750 /etc/kymo-lab/manifests
chmod 0640 /etc/kymo-lab/manifests/*.json 2>/dev/null || true
chgrp kymo-lab /etc/kymo-lab/manifests/*.json 2>/dev/null || true
# Verifier definitions (verifier.json and the check scripts) and the SSH keys they use.
install -d -m 0750 -o root -g kymo-lab /etc/kymo-lab/verifiers
install -d -m 0750 -o root -g kymo-lab /etc/kymo-lab/keys
install -d -m 0750 -o kymo-lab -g kymo-lab /var/lib/kymo-lab
# Virtual machine images go here (readable by the service, writable by root).
install -d -m 0755 /var/lib/kymo-lab/images 2>/dev/null || true
# Example manifests, never active until renamed to .json.
for example in "$HERE"/manifests/*.json.example; do
    [ -f "$example" ] || continue
    [ -f "/etc/kymo-lab/manifests/$(basename "$example")" ] || install -m 0640 -o root -g kymo-lab "$example" /etc/kymo-lab/manifests/
done
# The Verifier template to copy from (verifier.json, a bash and a python script).
if [ -d "$HERE/verifiers/template" ] && [ ! -d /etc/kymo-lab/verifiers/template ]; then
    cp -r "$HERE/verifiers/template" /etc/kymo-lab/verifiers/template
    chown -R root:kymo-lab /etc/kymo-lab/verifiers/template
    chmod -R g+rX,o-rwx /etc/kymo-lab/verifiers/template
fi

if [ -f /etc/kymo-lab/lab.env ]; then
    say "Keeping the existing settings and token in /etc/kymo-lab/lab.env"
    VPN_HOST=$(sed -n 's/^KYMO_LAB_VPN_HOST=//p' /etc/kymo-lab/lab.env | head -n 1)
    if [ -n "$VPN_HOST" ]; then
        VPN=1
        VPN_PORT=$(sed -n 's/^KYMO_LAB_VPN_PORT=//p' /etc/kymo-lab/lab.env | head -n 1)
        [ -n "$VPN_PORT" ] || VPN_PORT=1194
        VPN_PORT_EXPLICIT=0
        configured_port=$VPN_PORT
        choose_vpn_port
        if [ "$VPN_PORT" -ne "$configured_port" ]; then
            save_existing_vpn_port
            say "Updated /etc/kymo-lab/lab.env to use UDP $VPN_PORT"
        fi
    else
        VPN=0
    fi
else
    [ "$VPN" -eq 0 ] || choose_vpn_port
    say "Writing /etc/kymo-lab/lab.env with a new token"
    TOKEN=$(openssl rand -hex 24)
    {
        echo "KYMO_LAB_TOKEN=$TOKEN"
        echo "KYMO_LAB_ADDR=0.0.0.0:$PORT"
        echo "KYMO_LAB_NAME=$(hostname)"
        echo "KYMO_LAB_MODE=$MODE"
        echo "KYMO_LAB_RUNTIMES=$RUNTIMES"
        echo "KYMO_LAB_IMAGES=/var/lib/kymo-lab/images"
        echo "KYMO_LAB_MAX_INSTANCES=100"
        echo "KYMO_LAB_TLS_NAMES=$PUBLIC_HOST"
        [ -z "$ALLOW_FROM" ] || echo "KYMO_LAB_ALLOW_FROM=$ALLOW_FROM"
        echo "KYMO_LAB_DATA=/var/lib/kymo-lab"
        echo "KYMO_LAB_MANIFESTS=/etc/kymo-lab/manifests"
        if [ "$VPN" -eq 1 ]; then
            echo "KYMO_LAB_VPN_HOST=$VPN_HOST"
            echo "KYMO_LAB_VPN_PORT=$VPN_PORT"
            echo "KYMO_LAB_VPN_POOL=$VPN_POOL"
        fi
    } > /etc/kymo-lab/lab.env
    chown root:kymo-lab /etc/kymo-lab/lab.env
    chmod 0640 /etc/kymo-lab/lab.env
fi

say "Installing the service"
[ -f "$HERE/kymo-lab.service" ] || die "kymo-lab.service not found next to this script"
# The unit names only the groups this machine has: systemd refuses a group that does not exist.
if [ -n "$GROUPS_LINE" ]; then
    sed "s/@SUPPLEMENTARY_GROUPS@/SupplementaryGroups=$GROUPS_LINE/" "$HERE/kymo-lab.service" > /etc/systemd/system/kymo-lab.service
else
    grep -v '@SUPPLEMENTARY_GROUPS@' "$HERE/kymo-lab.service" > /etc/systemd/system/kymo-lab.service
fi
chmod 0644 /etc/systemd/system/kymo-lab.service
systemctl daemon-reload
systemctl enable kymo-lab >/dev/null 2>&1
systemctl restart kymo-lab

# Keep lab networks away from this machine itself (see the script). Only with the VPN, where a
# player can have root in a challenge machine on a bridge whose address is this host.
if [ "$VPN" -eq 1 ] && [ -f "$HERE/kymo-lab-firewall" ] && command -v iptables >/dev/null 2>&1; then
    say "Installing the lab network firewall rule"
    install -m 0755 "$HERE/kymo-lab-firewall" /usr/local/sbin/kymo-lab-firewall
    install -m 0644 "$HERE/kymo-lab-firewall.service" /etc/systemd/system/kymo-lab-firewall.service
    systemctl daemon-reload
    systemctl enable kymo-lab-firewall >/dev/null 2>&1
    systemctl restart kymo-lab-firewall || say "could not apply the firewall rule: run /usr/local/sbin/kymo-lab-firewall $VPN_POOL by hand"
fi

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
$( if [ "$VPN" -eq 1 ]; then cat <<VPNNOTE
  2. Open UDP $VPN_PORT to players: that is the VPN gateway, and the only port players need. No
     instance is published on any port, so nothing vulnerable is reachable from the internet.
     In the platform, switch the event to the VPN (the event's lab host settings).
VPNNOTE
else cat <<PORTNOTE
  2. Open the instance ports to players: Docker publishes each instance on a high port, by default
     32768-60999 (cat /proc/sys/net/ipv4/ip_local_port_range). Anything on those ports is reachable
     from the internet: prefer the VPN (run this again without --no-vpn).
PORTNOTE
fi )
  3. Put the challenge images on this machine and describe each one in
     /etc/kymo-lab/manifests/NAME.json (examples ending in .json.example are there):
       containers:        docker pull IMAGE   (or docker build)
       virtual machines:  copy the .ova or disk file to /var/lib/kymo-lab/images, runtime "vm" in
                          the manifest
     then restart (systemctl restart kymo-lab) and verify: kymo-lab check

Logs: journalctl -u kymo-lab -f      Upgrade: run this script again from a newer release.
The token is in /etc/kymo-lab/lab.env, readable only by root and the service.
DONE
