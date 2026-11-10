// Tests of the bridge against a stub MCP server (Node's http module).
//   node --test claude-desktop/test/
'use strict';

const assert = require('node:assert/strict');
const http = require('node:http');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { test, afterEach } = require('node:test');

// Stub servers and child processes of the current test, closed even when it fails.
const cleanups = [];
afterEach(async () => {
  while (cleanups.length) {
    await cleanups.pop()();
  }
});

const { Bridge, SseParser, parseServerUrl } = require('../server/bridge.js');

const BRIDGE = path.join(__dirname, '..', 'server', 'bridge.js');
const PROTOCOL = '2025-06-18';

// A stub Streamable HTTP server. reply: 'json' or 'sse'.
function stubServer({ reply = 'sse', sessions = true } = {}) {
  const state = { requests: [], deletes: [], liveSessions: new Set(), next: 1 };
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (chunk) => (body += chunk));
    req.on('end', () => {
      const session = req.headers['mcp-session-id'];
      if (req.method === 'DELETE') {
        state.deletes.push(session);
        state.liveSessions.delete(session);
        res.writeHead(200).end();
        return;
      }
      const message = JSON.parse(body);
      state.requests.push({ message, headers: req.headers });
      if (message.method !== 'initialize' && sessions && !state.liveSessions.has(session)) {
        res.writeHead(404).end('session not found');
        return;
      }
      if (!('id' in message)) {
        res.writeHead(202).end();
        return;
      }
      const headers = {};
      let result;
      if (message.method === 'initialize') {
        if (sessions) {
          const id = `s${state.next++}`;
          state.liveSessions.add(id);
          headers['Mcp-Session-Id'] = id;
        }
        result = { protocolVersion: PROTOCOL, capabilities: {}, serverInfo: { name: 'stub', version: '0' } };
      } else if (message.method === 'tools/list') {
        result = { tools: [{ name: 'list_decks', inputSchema: { type: 'object' } }] };
      } else if (message.method === 'big') {
        result = { text: 'x'.repeat(300 * 1024) };
      } else if (message.method === 'boom') {
        res.writeHead(500).end('internal trouble');
        return;
      } else {
        result = { echo: message.method };
      }
      const answer = JSON.stringify({ jsonrpc: '2.0', id: message.id, result });
      if (reply === 'sse') {
        res.writeHead(200, { ...headers, 'Content-Type': 'text/event-stream' });
        // Split across writes, as a real stream may arrive.
        const event = `event: message\ndata: ${answer}\n\n`;
        res.write(event.slice(0, 10));
        setTimeout(() => res.end(event.slice(10)), 5);
      } else {
        res.writeHead(200, { ...headers, 'Content-Type': 'application/json' });
        res.end(answer);
      }
    });
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const url = new URL(`http://127.0.0.1:${server.address().port}/mcp/`);
      let closed = false;
      const close = () => {
        if (closed) return Promise.resolve();
        closed = true;
        server.closeAllConnections();
        return new Promise((r) => server.close(r));
      };
      cleanups.push(close);
      resolve({ url, state, close, forget: () => state.liveSessions.clear() });
    });
  });
}

function collect() {
  const lines = [];
  return { lines, write: (line) => lines.push(JSON.parse(line)) };
}

const initialize = { jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: PROTOCOL, capabilities: {}, clientInfo: { name: 't', version: '0' } } };

test('the address must be http or https', () => {
  assert.equal(parseServerUrl(' http://192.0.2.10:3141/mcp/ ').href, 'http://192.0.2.10:3141/mcp/');
  assert.throws(() => parseServerUrl(''), /no server address/);
  assert.throws(() => parseServerUrl('192.0.2.10:3141'), /not an http or https address|not an address/);
  assert.throws(() => parseServerUrl('ftp://host/mcp/'), /not an http or https address/);
});

test('server-sent events are split into their data, across chunks', () => {
  const seen = [];
  const parser = new SseParser((d) => seen.push(d));
  parser.push('event: message\r\nda');
  parser.push('ta: {"a":1}\r\n\r\n: a comment\n\ndata: line1\ndata: line2\n\n');
  parser.push('data: {"last":true}');
  parser.finish();
  assert.deepEqual(seen, ['{"a":1}', 'line1\nline2', '{"last":true}']);
});

for (const reply of ['sse', 'json']) {
  test(`a ${reply} reply is passed on, with the session and the protocol version`, async () => {
    const server = await stubServer({ reply });
    const out = collect();
    const bridge = new Bridge(server.url, { write: out.write });
    await bridge.handle(initialize);
    await bridge.handle({ jsonrpc: '2.0', method: 'notifications/initialized' });
    await bridge.handle({ jsonrpc: '2.0', id: 1, method: 'tools/list' });
    assert.deepEqual(out.lines.map((m) => m.id), [0, 1]);
    assert.equal(out.lines[0].result.protocolVersion, PROTOCOL);
    assert.equal(out.lines[1].result.tools[0].name, 'list_decks');
    const later = server.state.requests.slice(1);
    assert.ok(later.every((r) => r.headers['mcp-session-id'] === 's1'));
    assert.ok(later.every((r) => r.headers['mcp-protocol-version'] === PROTOCOL));
    assert.match(server.state.requests[0].headers.accept, /application\/json/);
    assert.match(server.state.requests[0].headers.accept, /text\/event-stream/);
    await server.close();
  });
}

test('a notification gets no reply of its own', async () => {
  const server = await stubServer();
  const out = collect();
  const bridge = new Bridge(server.url, { write: out.write });
  await bridge.handle(initialize);
  await bridge.handle({ jsonrpc: '2.0', method: 'notifications/initialized' });
  assert.equal(out.lines.length, 1);
  await server.close();
});

test('requests sent before the initialize reply wait for it', async () => {
  const server = await stubServer();
  const out = collect();
  const bridge = new Bridge(server.url, { write: out.write });
  const early = bridge.handle({ jsonrpc: '2.0', id: 7, method: 'tools/list' });
  await bridge.handle(initialize);
  await early;
  assert.equal(server.state.requests[1].headers['mcp-session-id'], 's1');
  assert.deepEqual(out.lines.map((m) => m.id), [0, 7]);
  await server.close();
});

test('a server error answers the request with an error that names the address', async () => {
  const server = await stubServer();
  const out = collect();
  const bridge = new Bridge(server.url, { write: out.write });
  await bridge.handle(initialize);
  await bridge.handle({ jsonrpc: '2.0', id: 2, method: 'boom' });
  const error = out.lines.find((m) => m.id === 2).error;
  assert.equal(error.code, -32000);
  assert.match(error.message, new RegExp(`${server.url.host}.*HTTP 500: internal trouble`));
  await server.close();
});

test('a server that forgot the session gets a new one, and the request is sent again', async () => {
  const server = await stubServer();
  const out = collect();
  const bridge = new Bridge(server.url, { write: out.write });
  await bridge.handle(initialize);
  server.forget();
  await bridge.handle({ jsonrpc: '2.0', id: 3, method: 'tools/list' });
  assert.equal(bridge.session, 's2');
  // The new session's initialize reply is not passed to the client.
  assert.deepEqual(out.lines.map((m) => m.id), [0, 3]);
  assert.ok(out.lines[1].result.tools);
  await server.close();
});

test('a refused connection answers at once', async () => {
  const server = await stubServer();
  const url = server.url;
  await server.close();
  const out = collect();
  const bridge = new Bridge(url, { write: out.write });
  const started = Date.now();
  await bridge.handle(initialize);
  assert.ok(Date.now() - started < 2000);
  assert.match(out.lines[0].error.message, /connection refused/);
});

test('a server that never answers is given up on in time', async () => {
  const silent = http.createServer(() => {});
  await new Promise((r) => silent.listen(0, '127.0.0.1', r));
  cleanups.push(async () => {
    silent.closeAllConnections();
    await new Promise((r) => silent.close(() => r()));
  });
  const url = new URL(`http://127.0.0.1:${silent.address().port}/mcp/`);
  const out = collect();
  const bridge = new Bridge(url, { write: out.write, initializeTimeoutSeconds: 1 });
  const started = Date.now();
  await bridge.handle(initialize);
  const took = Date.now() - started;
  assert.ok(took >= 900 && took < 3000, `took ${took} ms`);
  assert.match(out.lines[0].error.message, /no answer in time/);
  silent.closeAllConnections();
  await new Promise((r) => silent.close(r));
});

test('as a process: stdio in and out, the session ended when stdin closes', async () => {
  const server = await stubServer();
  const child = spawn(process.execPath, [BRIDGE], {
    env: { ...process.env, SLIPCARD_MCP_URL: server.url.href },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  cleanups.push(async () => child.kill());
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (d) => (stdout += d));
  child.stderr.on('data', (d) => (stderr += d));
  child.stdin.write(`${JSON.stringify(initialize)}\n`);
  child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
  child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list' })}\n`);
  // Give the requests time to go out before stdin closes.
  await new Promise((r) => setTimeout(r, 300));
  child.stdin.end();
  const code = await new Promise((r) => child.on('exit', r));
  assert.equal(code, 0);
  const replies = stdout.trim().split('\n').map((l) => JSON.parse(l));
  assert.deepEqual(replies.map((m) => m.id), [0, 1]);
  assert.deepEqual(server.state.deletes, ['s1']);
  assert.match(stderr, /connected to/);
  await server.close();
});

test('as a process: a large reply reaches stdout whole when stdin closes at once', async () => {
  const server = await stubServer();
  const child = spawn(process.execPath, [BRIDGE, server.url.href], { stdio: ['pipe', 'pipe', 'pipe'] });
  cleanups.push(async () => child.kill());
  const chunks = [];
  child.stdout.on('data', (d) => chunks.push(d));
  child.stdin.write(`${JSON.stringify(initialize)}\n`);
  child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'big' })}\n`);
  child.stdin.end();
  const code = await new Promise((r) => child.on('close', r));
  assert.equal(code, 0);
  const replies = Buffer.concat(chunks).toString().trim().split('\n').map((l) => JSON.parse(l));
  assert.equal(replies.find((m) => m.id === 1).result.text.length, 300 * 1024);
  await server.close();
});

test('as a process: no address is a clear message and exit 1', async () => {
  const child = spawn(process.execPath, [BRIDGE], {
    env: { ...process.env, SLIPCARD_MCP_URL: '' },
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  let stderr = '';
  child.stderr.on('data', (d) => (stderr += d));
  const code = await new Promise((r) => child.on('exit', r));
  assert.equal(code, 1);
  assert.match(stderr, /no server address/);
});
