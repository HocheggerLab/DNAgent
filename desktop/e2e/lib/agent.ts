// Plays a live agent: MCP tool calls through the real `dnagent mcp` relay to the agent
// socket the e2e server opens for each browser session (as the desktop app does).
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { BINARY, CLI_ENV, REPO_ROOT } from './cli.ts';

/** Folder of per-session agent sockets (short: unix socket paths are limited to ~100 bytes). */
export const AGENT_SOCKETS = '/tmp/dnagent-e2e-1431';
export const agentSocket = (session: string) => `${AGENT_SOCKETS}/${session}.sock`;

const TIMEOUT_MS = 10_000;

/** Wait until the session's agent socket exists (it opens on the first view report). */
export async function waitForSocket(path: string): Promise<void> {
  const deadline = Date.now() + TIMEOUT_MS;
  while (!existsSync(path)) {
    if (Date.now() > deadline) throw new Error(`agent socket ${path} never appeared: did the page report its view?`);
    await new Promise(done => setTimeout(done, 25));
  }
}

/** One MCP session through `dnagent mcp`: initialize, call `tool`, return its JSON result. */
export async function callAgentTool(socket: string, tool: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await waitForSocket(socket);
  const relay = spawn(BINARY, ['mcp', '--socket', socket], { cwd: REPO_ROOT, env: CLI_ENV, stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = '';
  relay.stderr.on('data', chunk => { stderr += String(chunk); });
  const lines = createInterface({ input: relay.stdout });
  const send = (message: unknown) => relay.stdin.write(`${JSON.stringify(message)}\n`);
  const responses = new Map<number, (message: Record<string, unknown>) => void>();
  lines.on('line', line => {
    const message = JSON.parse(line) as Record<string, unknown>;
    responses.get(message.id as number)?.(message);
  });
  const request = (id: number, method: string, params: unknown) => new Promise<Record<string, unknown>>((done, fail) => {
    const timer = setTimeout(() => fail(new Error(`no answer to ${method} within ${TIMEOUT_MS} ms; relay stderr: ${stderr}`)), TIMEOUT_MS);
    responses.set(id, message => { clearTimeout(timer); done(message); });
    relay.once('exit', code => fail(new Error(`dnagent mcp exited (${code}) before answering ${method}: ${stderr}`)));
    send({ jsonrpc: '2.0', id, method, params });
  });
  try {
    await request(1, 'initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'dnagent-e2e', version: '0' } });
    send({ jsonrpc: '2.0', method: 'notifications/initialized' });
    const response = await request(2, 'tools/call', { name: tool, arguments: args });
    const error = response.error as { message?: string } | undefined;
    if (error) throw new AgentToolError(error.message ?? JSON.stringify(error));
    const result = response.result as { isError?: boolean; content?: { text?: string }[] };
    const text = result.content?.[0]?.text ?? '';
    if (result.isError) throw new AgentToolError(text);
    return JSON.parse(text);
  } finally {
    relay.stdin.end();
    lines.close();
  }
}

export class AgentToolError extends Error {}
