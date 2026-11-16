// The Slipcard Anki bridge: lets an MCP client that speaks stdio (Claude
// Desktop runs extensions that way) use an MCP server over Streamable HTTP,
// such as the one on Slipcard's Anki server. No dependencies: Node 18 or
// newer, with its built-in fetch.
//
//   SLIPCARD_MCP_URL=http://192.0.2.10:3141/mcp/ node index.js
//   node index.js http://192.0.2.10:3141/mcp/
//
// stdin and stdout carry newline-delimited JSON-RPC messages (the MCP stdio
// transport). Each message is POSTed to the URL; the reply, plain JSON or a
// stream of server-sent events, goes to stdout. Diagnostics go to stderr,
// which Claude Desktop keeps in the extension's log.
'use strict';

const readline = require('node:readline');

const NAME = 'slipcard-anki-bridge';
const ACCEPT = 'application/json, text/event-stream';
// The longest wait for a reply to start; a tool call may take a while.
const DEFAULT_TIMEOUT_SECONDS = 60;
// The first contact fails fast, so a wrong address is reported in seconds.
const INITIALIZE_TIMEOUT_SECONDS = 15;

function log(text) {
  process.stderr.write(`${NAME}: ${text}\n`);
}

// Returns the URL, or throws an Error whose message says what is wrong.
function parseServerUrl(raw) {
  const text = (raw || '').trim();
  if (!text) {
    throw new Error('no server address: set the Anki server MCP address in the extension settings');
  }
  let url;
  try {
    url = new URL(text);
  } catch {
    throw new Error(`not an address: "${text}" (expected http://host:3141/mcp/ or https://host/mcp/)`);
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error(`not an http or https address: "${text}"`);
  }
  return url;
}

// Splits a server-sent events stream into the data of its events.
class SseParser {
  constructor(onData) {
    this.onData = onData;
    this.buffer = '';
  }

  push(chunk) {
    this.buffer += chunk;
    let end;
    while ((end = this.buffer.search(/\r?\n\r?\n/)) !== -1) {
      const block = this.buffer.slice(0, end);
      this.buffer = this.buffer.slice(end).replace(/^\r?\n\r?\n/, '');
      this.event(block);
    }
  }

  finish() {
    if (this.buffer.trim()) {
      this.event(this.buffer);
    }
    this.buffer = '';
  }

  event(block) {
    const data = [];
    for (const line of block.split(/\r?\n/)) {
      if (line.startsWith('data:')) {
        data.push(line.slice(5).replace(/^ /, ''));
      }
    }
    if (data.length) {
      this.onData(data.join('\n'));
    }
  }
}

function isRequest(message) {
  return message && typeof message === 'object' && 'method' in message && 'id' in message;
}

function describe(error) {
  if (error && error.name === 'TimeoutError') {
    return 'no answer in time';
  }
  const cause = error && error.cause;
  if (cause && cause.code) {
    const codes = {
      ECONNREFUSED: 'connection refused (is the server running?)',
      ENOTFOUND: 'host name not found',
      EHOSTUNREACH: 'host unreachable',
      ENETUNREACH: 'network unreachable',
      ECONNRESET: 'connection reset',
      UND_ERR_CONNECT_TIMEOUT: 'no connection in time (is the address right?)',
    };
    return codes[cause.code] || `${cause.code}${cause.message ? `: ${cause.message}` : ''}`;
  }
  return (error && error.message) || String(error);
}

class Bridge {
  constructor(url, options = {}) {
    this.url = url;
    this.write = options.write || ((line) => process.stdout.write(line));
    this.fetch = options.fetch || globalThis.fetch;
    this.timeoutMs = (options.timeoutSeconds || DEFAULT_TIMEOUT_SECONDS) * 1000;
    this.initializeTimeoutMs = (options.initializeTimeoutSeconds || INITIALIZE_TIMEOUT_SECONDS) * 1000;
    this.session = null;
    this.protocolVersion = null;
    this.initializeMessage = null;
    this.ready = new Promise((resolve) => {
      this.markReady = resolve;
    });
  }

  // Handles one message from the client.
  async handle(message) {
    if (Array.isArray(message)) {
      await Promise.all(message.map((m) => this.handle(m)));
      return;
    }
    if (message && message.method === 'initialize') {
      this.initializeMessage = message;
      try {
        await this.send(message, { initialize: true });
      } finally {
        this.markReady();
      }
      return;
    }
    await this.ready;
    await this.send(message);
  }

  headers() {
    const headers = { 'Content-Type': 'application/json', Accept: ACCEPT };
    if (this.session) {
      headers['Mcp-Session-Id'] = this.session;
    }
    if (this.protocolVersion) {
      headers['MCP-Protocol-Version'] = this.protocolVersion;
    }
    return headers;
  }

  // POSTs one message. initialize: the first contact; silent: the replies
  // are not passed to the client (a new session started by the bridge).
  async send(message, { initialize = false, silent = false, retried = false } = {}) {
    let response;
    try {
      response = await this.fetch(this.url, {
        method: 'POST',
        headers: this.headers(),
        body: JSON.stringify(message),
        signal: AbortSignal.timeout(initialize ? this.initializeTimeoutMs : this.timeoutMs),
      });
    } catch (error) {
      return this.fail(message, describe(error), silent);
    }

    if (response.status === 404 && this.session && !initialize && !retried) {
      // The server forgot the session, for example after a restart.
      await response.body?.cancel();
      log('the server ended the session; starting a new one');
      this.session = null;
      if (await this.restartSession()) {
        return this.send(message, { silent, retried: true });
      }
      return this.fail(message, 'the server ended the session and a new one could not start', silent);
    }

    if (initialize) {
      const session = response.headers.get('mcp-session-id');
      if (session) {
        this.session = session;
      }
    }

    if (response.status === 202 || response.status === 204) {
      await response.body?.cancel();
      return true;
    }
    if (!response.ok) {
      const text = (await response.text().catch(() => '')).trim().slice(0, 200);
      return this.fail(message, `HTTP ${response.status}${text ? `: ${text}` : ''}`, silent);
    }

    const type = response.headers.get('content-type') || '';
    const deliver = (data) => this.deliver(data, { initialize, silent });
    if (type.includes('text/event-stream')) {
      const parser = new SseParser(deliver);
      const decoder = new TextDecoder();
      for await (const chunk of response.body) {
        parser.push(decoder.decode(chunk, { stream: true }));
      }
      parser.push(decoder.decode());
      parser.finish();
    } else {
      const text = await response.text();
      if (text.trim()) {
        deliver(text);
      }
    }
    if (initialize && !silent) {
      log(`connected to ${this.url.href}${this.session ? ' (session started)' : ''}`);
    }
    return true;
  }

  // One JSON-RPC message (or a batch) from the server.
  deliver(data, { initialize, silent }) {
    let parsed;
    try {
      parsed = JSON.parse(data);
    } catch {
      log(`skipped a reply that is not JSON: ${data.slice(0, 120)}`);
      return;
    }
    for (const message of Array.isArray(parsed) ? parsed : [parsed]) {
      if (initialize && message && message.result && message.result.protocolVersion) {
        this.protocolVersion = message.result.protocolVersion;
      }
      if (!silent) {
        this.write(`${JSON.stringify(message)}\n`);
      }
    }
  }

  // Answers a request that could not reach the server, so the client shows
  // the reason instead of waiting.
  fail(message, reason, silent) {
    log(`${this.url.href}: ${reason}`);
    if (!silent && isRequest(message)) {
      this.write(
        `${JSON.stringify({
          jsonrpc: '2.0',
          id: message.id,
          error: { code: -32000, message: `Slipcard Anki server at ${this.url.href}: ${reason}` },
        })}\n`,
      );
    }
    return false;
  }

  async restartSession() {
    if (!this.initializeMessage) {
      return false;
    }
    const restart = { ...this.initializeMessage, id: `${NAME}-restart-${Date.now()}` };
    const started = await this.send(restart, { initialize: true, silent: true });
    if (!started || !this.session) {
      return false;
    }
    await this.send({ jsonrpc: '2.0', method: 'notifications/initialized' }, { silent: true, retried: true });
    return true;
  }

  // Ends the session on the server; errors do not matter any more.
  async close() {
    if (!this.session) {
      return;
    }
    try {
      const response = await this.fetch(this.url, {
        method: 'DELETE',
        headers: this.headers(),
        signal: AbortSignal.timeout(2000),
      });
      await response.body?.cancel();
    } catch {
      // The server may be gone already.
    }
  }
}

function main() {
  let url;
  try {
    url = parseServerUrl(process.argv[2] || process.env.SLIPCARD_MCP_URL);
  } catch (error) {
    log(error.message);
    process.exit(1);
  }
  const seconds = Number(process.env.SLIPCARD_MCP_TIMEOUT) || DEFAULT_TIMEOUT_SECONDS;
  const bridge = new Bridge(url, { timeoutSeconds: seconds });
  const pending = new Set();

  const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  lines.on('line', (line) => {
    if (!line.trim()) {
      return;
    }
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      log(`skipped a line from the client that is not JSON: ${line.slice(0, 120)}`);
      return;
    }
    const work = bridge
      .handle(message)
      .catch((error) => log(`unexpected: ${error && error.stack ? error.stack : error}`))
      .finally(() => pending.delete(work));
    pending.add(work);
  });

  let closing = false;
  const finish = async () => {
    if (closing) {
      return;
    }
    closing = true;
    await Promise.allSettled([...pending]);
    await bridge.close();
    // stdout to a pipe is asynchronous on macOS: exiting before it drains
    // would cut a long reply short.
    await new Promise((resolve) => process.stdout.write('', resolve));
    process.exit(0);
  };
  lines.on('close', finish);
  process.on('SIGTERM', finish);
  process.on('SIGINT', finish);
  process.stdout.on('error', () => process.exit(0));
}

// Started by index.js, the extension's entry point; loading this file runs
// nothing, so the tests can load it.
module.exports = { Bridge, SseParser, parseServerUrl, describe, main };
