#!/bin/sh
set -e
if [ ! -f /vpn/server.conf ]; then
    echo "kymo-gateway: /vpn/server.conf is missing; start the Kymo server first, it writes the config" >&2
    exit 1
fi
mkdir -p /dev/net
[ -e /dev/net/tun ] || mknod /dev/net/tun c 10 200
# Deny everything until the generated rules are in place.
iptables -P FORWARD DROP
[ -f /vpn/rules.sh ] && sh /vpn/rules.sh
exec openvpn --config /vpn/server.conf
