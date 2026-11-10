# Slipcard's Anki server: a headless desktop Anki with two add-ons,
# AnkiConnect (the API Slipcard talks to) and AnkiMCP (an MCP server for AI
# assistants). One Anki process serves both on one collection; two processes
# cannot open the same collection.
#
# The base image is ankimcp/headless-anki (AGPL-3.0): Anki from PyPI with
# the Qt libraries, TigerVNC and openbox. It is pinned by digest, and the
# add-ons by SHA-256. The image is meant to be built locally, not published.
ARG BASE_IMAGE=ghcr.io/ankimcp/headless-anki:x11-vnc-v1.9.0@sha256:e075eeab3dbc98e137720d846a07bd3ba4defe13b62d7a3bc0560f3dc8bbf98d
FROM ${BASE_IMAGE}

# AnkiConnect (GPL-3.0) as AnkiWeb serves it for Anki 26.9.4. AnkiWeb only
# serves the current version of an add-on, so a new AnkiConnect release
# changes the hash and stops the build: look at the change, then set the new
# hash here.
ARG ANKICONNECT_URL="https://ankiweb.net/shared/download/2055492159?v=2.1&p=260904"
ARG ANKICONNECT_SHA256=456a546fd9e1bf53962c70618f7d72d10b1768690ce88fa5c9c6909ae5e9e1ec
# AnkiMCP (AGPL-3.0) from its GitHub release.
ARG ANKIMCP_VERSION=0.41.1
ARG ANKIMCP_SHA256=4816108718c3749b8df4749ed166fe8825d0a0e0c6256abe99fa23c7d23aca58

RUN set -eu; \
    mkdir -p /opt/addons; cd /opt/addons; \
    curl -fsSL -o ankiconnect.zip "$ANKICONNECT_URL"; \
    curl -fsSL -o ankimcp.zip \
      "https://github.com/ankimcp/anki-mcp-server-addon/releases/download/v${ANKIMCP_VERSION}/anki_mcp_server.ankiaddon"; \
    echo "$ANKICONNECT_SHA256  ankiconnect.zip" | sha256sum -c -; \
    echo "$ANKIMCP_SHA256  ankimcp.zip" | sha256sum -c -; \
    unzip -q ankiconnect.zip -d 2055492159; \
    unzip -q ankimcp.zip -d anki_mcp_server; \
    rm ankiconnect.zip ankimcp.zip

COPY entrypoint.sh configure.py restore.py /opt/slipcard/
RUN chmod 0755 /opt/slipcard/entrypoint.sh

EXPOSE 8765 3141 5900
ENTRYPOINT ["/opt/slipcard/entrypoint.sh"]
