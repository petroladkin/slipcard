# Slipcard Anki server: Claude Desktop extension

Connects Claude Desktop to the MCP server of a Slipcard Anki server
([docs/server.md](../docs/server.md)) on your network, so Claude can work
with your decks, notes and cards.

Claude Desktop runs extensions as local programs that speak MCP over stdio.
The server speaks MCP over HTTP, and a connector added by URL in Claude
Desktop is reached from Anthropic's side, not from your computer, so it
cannot reach an address in your network. This extension is a small bridge
between the two: `server/bridge.js`, plain Node with no dependencies,
started by `server/index.js`.

## Build

```sh
claude-desktop/build.sh          # checks, tests, then claude-desktop/slipcard-anki.mcpb
```

## Install

1. Open `slipcard-anki.mcpb` with Claude Desktop (double-click it).
2. It asks for the **Anki server MCP address**: the server's address with
   `/mcp/` at the end, for example `http://192.0.2.10:3141/mcp/`, or
   `https://anki.example.com/mcp/` when the server has HTTPS.
3. Enable it. In a new chat, Claude lists the Anki tools (decks, notes,
   cards, reviews).
4. On macOS, allow Claude to reach devices on your local network if macOS
   asks: until then a connection to an address in your network fails, and
   the bridge reports "host unreachable".

To change the address later, open the extension's settings in Settings →
Extensions. To install a newer build, remove the extension there first,
then open the new file; it asks for the address again.

The server has no API key: anyone in your network who can reach it can
change your collection, with or without this extension.

## When it does not connect

The bridge answers Claude with the reason: "connection refused" (the server
is not running), "no connection in time" (wrong address or the host is
off), "host name not found", "host unreachable" (on macOS, often the Local
Network permission: System Settings → Privacy & Security → Local Network),
or an HTTP status (for example 404 for a wrong path; the path is `/mcp/`).
If the server restarts, the bridge starts a new session by itself.

On macOS the logs are in `~/Library/Logs/Claude/`: the reason is in the
extension's log, `mcp-server-Slipcard Anki server.log`. When Claude Desktop
runs the extension on its built-in Node, the bridge's own lines (such as
"connected to …") are in `main.log`, marked `[UtilityProcess stderr]`.

## Run it by hand

```sh
SLIPCARD_MCP_URL=http://192.0.2.10:3141/mcp/ node claude-desktop/server/index.js
```

It reads newline-delimited JSON-RPC on stdin and writes the replies to
stdout. `SLIPCARD_MCP_TIMEOUT` (seconds, default 60) bounds the wait for a
reply to start.
