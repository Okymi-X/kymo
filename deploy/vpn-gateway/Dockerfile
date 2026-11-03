# The OpenVPN gateway used for VPN mode events. It holds no secrets of its own: Kymo writes
# the config, certificates and firewall script to a directory that is mounted at /vpn.
FROM docker.io/library/alpine:3
RUN apk add --no-cache openvpn iptables iproute2
COPY entrypoint.sh /entrypoint.sh
RUN chmod 0755 /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
