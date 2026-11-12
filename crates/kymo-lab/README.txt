kymo-lab: the Kymo lab host

  kymo-lab               run (configured by KYMO_LAB_* environment variables)
  kymo-lab fingerprint   print the certificate fingerprint the platform pins
  kymo-lab check         verify configuration, Docker access, manifests, flag delivery and Verifier definitions
  kymo-lab verifier test DIR --target IP [--ports 22,80] [--lib DIR] [--strict]
                         run a Verifier definition against a running machine, with no player
  kymo-lab version

Environment:
  KYMO_LAB_TOKEN          required, 24+ characters; the platform authenticates with it
  KYMO_LAB_TOKEN_FILE     read the token from a file instead
  KYMO_LAB_ADDR           listen address (default 0.0.0.0:7400)
  KYMO_LAB_NAME           the lab host's name (default: hostname)
  KYMO_LAB_DATA           state and certificate directory (default /var/lib/kymo-lab)
  KYMO_LAB_MANIFESTS      manifest directory (default /etc/kymo-lab/manifests)
  KYMO_LAB_MODE           manifest (default) or open
  KYMO_LAB_MAX_INSTANCES  most instances at once (default 100)
  KYMO_LAB_ALLOW_FROM     comma separated addresses or CIDR ranges allowed to call the API
  KYMO_LAB_BIND           address instance ports are published on (default 0.0.0.0)
  KYMO_LAB_TLS            on (default) or off
  KYMO_LAB_TLS_NAMES      extra host names or IPs for the generated certificate
  KYMO_LAB_TLS_CERT/KEY   use this certificate and key instead of a generated one
  KYMO_LAB_VERIFIERS      Verifier definitions directory (default /etc/kymo-lab/verifiers)
  KYMO_LAB_VERIFIER_IMAGE the Verifier container image (default localhost/kymo/verifier:1)

See the Kymo README, "Separate lab host".
