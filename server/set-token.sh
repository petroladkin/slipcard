#!/usr/bin/env bash
# Asks for the Cloudflare API token of the tls profile and stores it in the
# .env next to this script (mode 0600). The token is read without echo and
# never printed. Run it in a terminal on the server host:
#
#   server/set-token.sh
set -euo pipefail

env_file="$(cd "$(dirname "$0")" && pwd)/.env"
[ -t 0 ] || { echo "run it in a terminal: it reads the token from the keyboard" >&2; exit 1; }

read -rsp "Cloudflare API token (DNS edit of the zone): " token
echo
[ -n "$token" ] || { echo "empty, nothing changed" >&2; exit 1; }
case "$token" in
  *[[:space:]]*) echo "the token has spaces in it, nothing changed" >&2; exit 1 ;;
esac

touch "$env_file"
chmod 600 "$env_file"
tmp=$(mktemp "$env_file.XXXXXX")
chmod 600 "$tmp"
grep -v '^CF_API_TOKEN=' "$env_file" > "$tmp" || true
printf 'CF_API_TOKEN=%s\n' "$token" >> "$tmp"
mv "$tmp" "$env_file"
unset token
echo "saved to $env_file (mode 600)"
