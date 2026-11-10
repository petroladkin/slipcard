# The Anki server

Slipcard needs an Anki with the AnkiConnect add-on somewhere. Usually that is
the desktop Anki on the same computer, left running in the background. The
server in `server/` is the other way: one Anki in Docker on a machine that is
always on, which Slipcard on several computers and AI assistants (through
MCP) share, so no Anki has to run on the desktops.

It is optional, and it is not a public service: read
[No protection](#no-protection) first.

## What runs

One container, one Anki process, one collection:

| Port | What | Used by |
|---|---|---|
| 8765 | AnkiConnect (the add-on's HTTP API) | Slipcard, other AnkiConnect tools |
| 3141 | MCP over Streamable HTTP, at `/mcp/` (the AnkiMCP add-on) | Claude Desktop (through the extension), Claude Code, other MCP clients |
| 5900 | VNC: Anki's window, no password | you, through an SSH tunnel, when you need the GUI |
| 443 | HTTPS for both (Caddy, the optional `tls` profile) | the same clients, encrypted |

The image is built from [ankimcp/headless-anki](https://github.com/ankimcp/headless-anki)
(the desktop Anki 26.9.4 from PyPI with Qt, TigerVNC and openbox), the
[AnkiConnect](https://ankiweb.net/shared/info/2055492159) add-on and the
[AnkiMCP](https://github.com/ankimcp/anki-mcp-server-addon) add-on. All three
are pinned (the base image by digest, the add-ons by SHA-256). They are
AGPL-3.0 (the image, AnkiMCP) and GPL-3.0 (AnkiConnect); none of their code
is in this repository, and the image is meant to be built locally, not
published.

Both add-ons live in the same Anki because only one process can open a
collection: a second one gets "Anki already open".

## No protection

There is no API key and there are no users. **Anyone who can reach these
ports can read, change and delete the whole collection**, through AnkiConnect,
through MCP, or through VNC. Run it only on a network you trust, never
publish the ports to the internet, and keep VNC on `127.0.0.1` (the default).
The `tls` profile adds encryption, not access control.

Two smaller guards stay on: the MCP add-on answers only requests whose
`Host` is one of `ANKI_SERVER_HOSTS` (against DNS rebinding from a browser),
and its most destructive tools (changing a note type, removing a field) are
hidden unless enabled in its settings.

## No AnkiWeb sync

This setup does not sync with AnkiWeb: the collection lives on the server
and in its backups. Reviews made through the server do not reach AnkiWeb, the
phone apps or a desktop Anki that syncs with AnkiWeb, and the other way round.
Pick one place to review: the server (with Slipcard and MCP), or AnkiWeb.

## Run it

Needs Docker with Compose v2, about 1 GB of RAM and 3 GB of disk (the image
is about 1.4 GB).

```sh
cd server
cp .env.example .env      # set ANKI_SERVER_HOSTS to this host's address
docker compose up -d --build
docker compose ps         # "healthy" after a minute
```

Check both ports from another computer (`192.0.2.10` stands for the
server's address):

```sh
curl -s -d '{"action":"version","version":6}' http://192.0.2.10:8765
# {"result": 6, "error": null}
curl -s -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}' \
  http://192.0.2.10:3141/mcp/
# event: message / data: {"jsonrpc":"2.0","id":1,"result":{... "serverInfo":{"name":"anki-mcp", ...
```

The first start creates the profile `ANKI_PROFILE` (an empty collection).
Put your cards on it with [Restore](#import-or-restore-a-collection).

### Settings (`server/.env`)

| Setting | Default | Meaning |
|---|---|---|
| `ANKI_SERVER_HOSTS` | (none) | host names or addresses clients use, comma-separated; the MCP server refuses others |
| `ANKI_PROFILE` | `User 1` | the Anki profile that Anki opens; a new name is created, empty, at the next start, and the other profiles stay in the data directory |
| `ANKI_DATA_DIR` | `./data` | the profile (collection, media, Anki's backups) and the add-ons' settings |
| `ANKI_BIND` | `0.0.0.0` | where 8765 and 3141 are published; `127.0.0.1` with the `tls` profile |
| `VNC_BIND` | `127.0.0.1` | where 5900 is published |
| `ANKI_TLS_NAME`, `CF_API_TOKEN`, `TLS_BIND` | (none), (none), `0.0.0.0` | the `tls` profile, below |

Settings apply at the next start (`docker compose up -d`).

## Point the clients at it

- **Slipcard**: in its settings file, `"ankiConnectUrl": "http://192.0.2.10:8765"`
  (or `https://<name>` with TLS), no `ankiConnectKey`. See
  [development.md](development.md#settings).
- **Claude Code**: `claude mcp add --transport http anki http://192.0.2.10:3141/mcp/`
  (or `https://<name>/mcp/`).
- **Claude Desktop**: the extension in `claude-desktop/`, which asks for the
  MCP address (`http://192.0.2.10:3141/mcp/`) when it is installed; see
  [claude-desktop/README.md](../claude-desktop/README.md).

## HTTPS (the `tls` profile)

Caddy in front of both, with a Let's Encrypt certificate by a DNS-01
challenge through Cloudflare, so the server needs no open port 80 and can
stay inside a private network:

1. A name in a Cloudflare zone, with an A record to the server's address
   (DNS only, not proxied), for example `anki.example.com`.
2. A Cloudflare API token (a custom token) with two permissions, Zone →
   DNS → Edit and Zone → Zone → Read, limited to that zone (Zone Resources:
   Include, Specific zone). Without Zone → Read Caddy cannot find the zone.
   On the server, `server/set-token.sh` asks for it without echo and writes
   it into `.env` (mode 600). Nothing shows while you paste, so paste once.
3. In `.env`: `ANKI_TLS_NAME=anki.example.com`, and `ANKI_BIND=127.0.0.1`
   so nothing reaches Anki around Caddy.
4. `docker compose --profile tls up -d --build`

AnkiConnect is then at `https://anki.example.com/`, the MCP server at
`https://anki.example.com/mcp/`.

The certificate takes under a minute: `docker compose logs caddy` shows
`certificate obtained successfully`. If it shows `Invalid format for
Authorization header` instead, the token in `.env` is not the one Cloudflare
issued (pasted twice, for example): run `set-token.sh` again, then
`docker compose --profile tls up -d --force-recreate caddy`. A computer that
looked the name up before its A record existed may keep getting "not found"
for up to 30 minutes, because resolvers cache the missing name for the
time the zone sets (Cloudflare: 1800 s).

## The GUI, through VNC

VNC listens on the server itself only. From your computer:

```sh
ssh -L 5900:127.0.0.1:5900 <server>
```

and open `vnc://127.0.0.1:5900` (macOS: Finder → Go → Connect to Server) or
any VNC viewer. No password is asked. Anki's window is there, with every
menu; closing it starts Anki again.

## Import or restore a collection

To put your collection on the server, export it from your desktop Anki:
File → Export → "Anki Collection Package (.colpkg)", with "Include media".
To go back to one of the server's backups, take a `.colpkg` from
`data/<profile>/backups/` or from where you copy them. Then, next to
`compose.yaml`:

```sh
server/restore.sh /path/to/collection.colpkg
```

It stops Anki, keeps the current collection as
`data/<profile>/backups/before-restore-<time>.anki2`, imports the package as
Anki's File → Import does, and starts Anki again.

### Move a deck to another profile

For example cards made in a test profile that belong in yours. With the old
profile open, export the deck through AnkiConnect (the path is inside the
container, where the data directory is `/data`), switch the profile, and
import it:

```sh
mkdir -p data/exports
curl -s http://192.0.2.10:8765 -d '{"action": "exportPackage", "version": 6,
  "params": {"deck": "My words", "path": "/data/exports/my-words.apkg", "includeSched": true}}'
# set ANKI_PROFILE in .env to the other profile, then (with --profile tls if you use it)
docker compose up -d
curl -s http://192.0.2.10:8765 -d '{"action": "importPackage", "version": 6,
  "params": {"path": "/data/exports/my-words.apkg"}}'
```

The `.apkg` is also a copy of the deck to keep. If the profile already has
a note type of the same name, Anki may import the deck's one under a new
name, such as `Basic+`. Slipcard's integration tests and test tools refuse
any profile but `Slipcard Test`, so they never touch a profile with your
cards.

## Backups

Anki keeps its own backups in `data/<profile>/backups/`: a `.colpkg` after
the collection changes, at most every 30 minutes by default, thinned out to
some daily, weekly and monthly ones. They are on the same disk as the
collection, so copy the newest one elsewhere every night, for example with
cron on the server host:

```sh
# m h dom mon dow  command
15 3 * * *  /path/to/server/backup.sh /path/to/server/data /mnt/backup/anki 7
```

`backup.sh <data dir> <target dir> [keep]` copies the newest backup and
keeps the newest `keep` copies (7 by default). It needs GNU `find`.

## Update

The versions are pinned in `server/Dockerfile` and `server/compose.yaml`.
After changing one: `docker compose up -d --build`. The collection, its
backups and the add-ons' settings in `data/` stay. AnkiWeb serves only the
newest AnkiConnect, so the build stops with a checksum error when a new
release appears; look at it and update `ANKICONNECT_SHA256`.

## Stop and remove

`docker compose down` stops it and keeps `data/`. Anki closes cleanly on
`docker compose stop` (it saves the collection), within the 30 s grace
period.
