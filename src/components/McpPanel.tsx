import { useCallback, useEffect, useRef, useState } from "react";
import { api, onMcpLog } from "../api";
import { Icon } from "./Icon";
import type { McpServer, McpStatus, ToolLog } from "../types";

interface Props {
  onClose: () => void;
  /** Rendered inside the Customise hub: fills it instead of covering the app. */
  embedded?: boolean;
}

const smallCls =
  "w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-none";

/** `KEY=value` per line, which is how everyone copies tokens into these files. */
function parsePairs(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const eq = trimmed.indexOf("=");
    if (eq === -1) continue;
    out[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
  }
  return out;
}

function pairsToText(pairs: Record<string, string>): string {
  return Object.entries(pairs)
    .map(([key, value]) => `${key}=${value}`)
    .join("\n");
}

function blankServer(): McpServer {
  return {
    name: "new-server",
    command: "npx",
    args: [],
    env: {},
    url: "",
    headers: {},
    enabled: true,
    trustReadOnly: false,
  };
}

/** `GET /mcp` reads badly but says "still starting", which is the honest state. */
function logLine(line: ToolLog): string {
  if (line.kind === "stderr") return line.detail;
  const call = `${line.tool}(${line.args})`;
  return line.ok ? `${call} → ${line.size} chars in ${line.ms}ms` : `${call} → ${line.detail}`;
}

export function McpPanel({ onClose, embedded = false }: Props) {
  const [tab, setTab] = useState<"servers" | "logs">("servers");
  const [servers, setServers] = useState<McpServer[]>([]);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [logs, setLogs] = useState<ToolLog[]>([]);
  const [logServer, setLogServer] = useState<string>("");
  const logEnd = useRef<HTMLDivElement | null>(null);

  const load = useCallback(async () => {
    try {
      const [list, snapshot, lines] = await Promise.all([
        api.mcpServers(),
        api.mcpStatus(),
        api.mcpLogs(),
      ]);
      setServers(list);
      setStatus(snapshot);
      setLogs(lines);
      setDirty(false);
    } catch (e) {
      setNote(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // New lines arrive on their own event, so the Logs tab is live rather than
  // refreshed on a timer. Only while the panel is open — nothing else shows it.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void onMcpLog((line) => setLogs((prev) => [...prev, line])).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    logEnd.current?.scrollIntoView({ block: "end" });
  }, [logs, tab]);

  const patch = (index: number, changes: Partial<McpServer>) => {
    setServers((prev) => prev.map((s, i) => (i === index ? { ...s, ...changes } : s)));
    setDirty(true);
  };

  const save = async () => {
    // A server is started one of two ways: a local command, or a remote url.
    const broken = servers.find(
      (s) => !s.name.trim() || (!s.command.trim() && !s.url?.trim()),
    );
    if (broken) {
      setNote("Every server needs a name and either a command or a url.");
      return;
    }
    setBusy(true);
    setNote("Restarting servers…");
    try {
      setStatus(await api.mcpSaveServers(servers));
      setDirty(false);
      setNote("Saved.");
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  };

  const restart = async (name: string) => {
    setBusy(true);
    setNote(`Restarting ${name}…`);
    try {
      setStatus(await api.mcpRestart(name));
      setNote(null);
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  };

  const reload = async () => {
    setBusy(true);
    try {
      await load();
      setNote("Re-read mcp.json.");
    } finally {
      setBusy(false);
    }
  };

  const statusOf = (name: string) => status?.servers.find((s) => s.name === name);

  return (
    <div
      className={
        embedded
          ? "flex h-full min-h-0 min-w-0 flex-1 flex-col bg-neutral-950"
          : "fixed inset-0 z-50 grid place-items-center bg-black/60 p-4"
      }
    >
      <div
        className={
          embedded
            ? "flex min-h-0 flex-1 flex-col overflow-hidden"
            : "flex max-h-[85vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-neutral-800 bg-neutral-950"
        }
      >
        <div className="flex items-center justify-between border-b border-neutral-800 px-5 py-3">
          <div>
            <h2 className="text-sm font-semibold">MCP servers</h2>
            <p className="text-[11px] text-neutral-500">
              {status
                ? `${status.runningCount} running · ${status.toolCount} tool${status.toolCount === 1 ? "" : "s"} offered to the model`
                : "Loading…"}
            </p>
          </div>
          {!embedded && (
            <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200" title="Close">
              <Icon name="close" size={15} />
            </button>
          )}
        </div>

        <div className="flex items-center gap-1 border-b border-neutral-800 px-5">
          {(["servers", "logs"] as const).map((name) => (
            <button
              key={name}
              onClick={() => setTab(name)}
              className={`-mb-px border-b-2 px-3 py-2 text-xs capitalize transition ${
                tab === name
                  ? "border-sky-500 text-neutral-100"
                  : "border-transparent text-neutral-500 hover:text-neutral-300"
              }`}
            >
              {name}
              {name === "logs" && logs.length > 0 && (
                <span className="ml-1.5 text-[10px] text-neutral-600">{logs.length}</span>
              )}
            </button>
          ))}
        </div>

        {tab === "servers" && (
          <div className="flex flex-wrap items-center gap-2 border-b border-neutral-800 px-5 py-2">
            <button
              onClick={save}
              disabled={busy || !dirty}
              className="rounded-lg bg-sky-600 px-3 py-1.5 text-xs font-medium text-accent-fg hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
            >
              Save &amp; restart all
            </button>
            <button
              onClick={reload}
              disabled={busy}
              className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
            >
              Re-read file
            </button>
            <button
              onClick={() => {
                setServers((prev) => [...prev, blankServer()]);
                setDirty(true);
              }}
              className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100"
            >
              + Add server
            </button>
            <span className="ml-auto text-[11px] text-neutral-600">{note}</span>
          </div>
        )}

        {tab === "logs" && (
          <div className="flex flex-wrap items-center gap-2 border-b border-neutral-800 px-5 py-2">
            <select
              value={logServer}
              onChange={(e) => setLogServer(e.target.value)}
              className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-none"
            >
              <option value="">All servers</option>
              {[...new Set(logs.map((line) => line.server))].map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <button
              onClick={() => setLogs([])}
              className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-300 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100"
            >
              Clear view
            </button>
            <span className="ml-auto text-[11px] text-neutral-600">
              What the servers have been doing, newest last. Clearing only hides them here.
            </span>
          </div>
        )}

        {tab === "logs" && (
          <div className="min-h-0 flex-1 overflow-y-auto bg-neutral-950 px-5 py-4 font-mono text-[11px] leading-relaxed">
            {logs.length === 0 && (
              <p className="py-6 text-center font-sans text-xs text-neutral-500">
                Nothing yet. Tool calls and anything a server writes to stderr appear here.
              </p>
            )}
            {logs
              .filter((line) => !logServer || line.server === logServer)
              .map((line, index) => (
                <div
                  key={index}
                  className={`flex gap-2 border-b border-neutral-900 py-1 ${
                    line.kind === "stderr"
                      ? "text-neutral-500"
                      : line.ok
                        ? "text-neutral-300"
                        : "text-red-400"
                  }`}
                >
                  <span className="shrink-0 text-neutral-700">
                    {new Date(line.at).toLocaleTimeString()}
                  </span>
                  <span className="shrink-0 text-neutral-600">{line.server}</span>
                  <span className="whitespace-pre-wrap break-all">{logLine(line)}</span>
                </div>
              ))}
            <div ref={logEnd} />
          </div>
        )}

        {tab === "servers" && (
        <div className="flex-1 space-y-3 overflow-y-auto px-5 py-4">
          {status?.configError && (
            <p className="rounded-lg border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">
              {status.configError}
            </p>
          )}

          <p className="text-[11px] text-neutral-600">
            Config file: <span className="font-mono">{status?.configPath}</span>
          </p>

          {servers.length === 0 && (
            <p className="py-6 text-center text-xs text-neutral-500">
              No servers yet. Add one — for example{" "}
              <span className="font-mono">npx -y @modelcontextprotocol/server-everything</span>.
            </p>
          )}

          {servers.map((server, index) => {
            const live = statusOf(server.name);
            return (
              <div
                key={index}
                className="space-y-2 rounded-xl border border-neutral-800 bg-neutral-900/40 p-3"
              >
                <div className="flex items-center gap-2">
                  <span
                    className={`h-2 w-2 shrink-0 rounded-full ${
                      live?.running
                        ? "bg-emerald-500"
                        : server.enabled
                          ? "bg-red-500"
                          : "bg-neutral-600"
                    }`}
                    title={
                      live?.running
                        ? "Running"
                        : server.enabled
                          ? "Failed to start"
                          : "Disabled"
                    }
                  />
                  <input
                    className={`${smallCls} flex-1`}
                    value={server.name}
                    onChange={(e) => patch(index, { name: e.target.value })}
                    placeholder="name"
                  />
                  <label className="flex shrink-0 items-center gap-1 text-[11px] text-neutral-400">
                    <input
                      type="checkbox"
                      checked={server.enabled}
                      onChange={(e) => patch(index, { enabled: e.target.checked })}
                    />
                    on
                  </label>
                  <button
                    onClick={() => restart(server.name)}
                    disabled={busy || !server.enabled}
                    className="shrink-0 rounded-md px-2 py-1 text-[11px] text-neutral-400 transition duration-150 hover:bg-neutral-800 hover:text-neutral-200 disabled:opacity-40"
                  >
                    Restart
                  </button>
                  <button
                    onClick={() => {
                      setServers((prev) => prev.filter((_, i) => i !== index));
                      setDirty(true);
                    }}
                    className="shrink-0 text-neutral-500 hover:text-red-400"
                    title="Remove"
                  >
                    <Icon name="close" />
                  </button>
                </div>

                <div className="flex gap-2">
                  <input
                    className={`${smallCls} flex-1 font-mono`}
                    value={server.command}
                    onChange={(e) => patch(index, { command: e.target.value })}
                    placeholder="command, e.g. npx or uvx"
                    spellCheck={false}
                    disabled={!!server.url?.trim()}
                  />
                  <input
                    className={`${smallCls} flex-[2] font-mono`}
                    value={server.args.join(" ")}
                    onChange={(e) =>
                      patch(index, { args: e.target.value.split(/\s+/).filter(Boolean) })
                    }
                    placeholder="arguments"
                    spellCheck={false}
                    disabled={!!server.url?.trim()}
                  />
                </div>

                <input
                  className={`${smallCls} w-full font-mono`}
                  value={server.url ?? ""}
                  onChange={(e) => patch(index, { url: e.target.value })}
                  placeholder="…or a remote URL, e.g. https://example.com/mcp (no child process)"
                  spellCheck={false}
                />

                <details className="text-[11px] text-neutral-500">
                  <summary className="cursor-pointer select-none">Environment</summary>
                  <textarea
                    className={`${smallCls} mt-1 h-16 resize-y font-mono`}
                    value={pairsToText(server.env)}
                    onChange={(e) => patch(index, { env: parsePairs(e.target.value) })}
                    placeholder="API_TOKEN=…&#10;one KEY=value per line"
                    spellCheck={false}
                  />
                </details>

                <details className="text-[11px] text-neutral-500">
                  <summary className="cursor-pointer select-none">HTTP headers</summary>
                  <textarea
                    className={`${smallCls} mt-1 h-16 resize-y font-mono`}
                    value={pairsToText(server.headers)}
                    onChange={(e) => patch(index, { headers: parsePairs(e.target.value) })}
                    placeholder="Authorization=Bearer …&#10;one Name=value per line, remote servers only"
                    spellCheck={false}
                  />
                </details>

                <label className="flex items-start gap-2 text-[11px] text-neutral-400">
                  <input
                    type="checkbox"
                    className="mt-0.5"
                    checked={server.trustReadOnly}
                    onChange={(e) => patch(index, { trustReadOnly: e.target.checked })}
                  />
                  <span>
                    Run tools this server marks as read-only without asking. Off by default: a
                    server describes its own tools, so only tick this for one you trust.
                  </span>
                </label>

                {live?.error && (
                  <p className="rounded-md border border-red-900/60 bg-red-950/40 px-2 py-1 text-[11px] text-red-300">
                    {live.error}
                  </p>
                )}
                {live?.running && (
                  <p className="text-[11px] text-neutral-600">
                    {live.toolCount} tool{live.toolCount === 1 ? "" : "s"} available
                  </p>
                )}
              </div>
            );
          })}
        </div>
        )}
      </div>
    </div>
  );
}
