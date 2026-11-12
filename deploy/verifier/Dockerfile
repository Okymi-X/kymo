# The Verifier container started next to a Responder instance. The lab host builds it once
# (install-lab.sh) and starts one per instance, on the player's lab network, with the
# organizer's definition directory mounted read-only at /verifier.
#
# The release puts the program and the helper libraries next to this file:
#   kymo-verifier   the web service
#   lib/            kymo_verify.sh and kymo_verify.py, the helpers check scripts use
#
# Build the binary on the same distribution family as the base image (or build it static), or
# it may not start here.
ARG BASE=debian:stable-slim
FROM ${BASE}
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        bash python3 curl openssh-client sshpass jq netcat-openbsd iproute2 ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home verifier
COPY kymo-verifier /usr/local/bin/kymo-verifier
COPY lib/ /usr/local/lib/kymo-verifier/
USER 10001
ENV KYMO_VERIFIER_DEFINITION=/verifier \
    KYMO_VERIFIER_LIB=/usr/local/lib/kymo-verifier \
    KYMO_VERIFIER_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/kymo-verifier"]
