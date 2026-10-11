import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import type { McpServer, McpStatus } from "../types";

interface Props {
  onClose: () => void;
}

const smallCls =
  "w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-none";

/** `KEY=value` per line, which is how everyone copies tokens into these files. */
function parseEnv(text: string): Record<string, string> {
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

function envToText(env: Record<string, string>): string {
  return Object.entries(env)
    .map(([key, value]) => `${key}=${value}`)
    .join("\n");
}

function blankServer(): McpServer {
  return {
    name: "new-server",
    command: "npx",
    args: [],
    env: {},
    enabled: true,
    trustReadOnly: false,
  };
}

export function McpPanel({ onClose }: Props) {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [list, snapshot] = await Promise.all([api.mcpServers(), api.mcpStatus()]);
      setServers(list);
      setStatus(snapshot);
      setDirty(false);
    } catch (e) {
      setNote(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const patch = (index: number, changes: Partial<McpServer>) => {
    setServers((prev) => prev.map((s, i) => (i === index ? { ...s, ...changes } : s)));
    setDirty(true);
  };

  const save = async () => {
    const broken = servers.find((s) => !s.name.trim() || !s.command.trim());
    if (broken) {
      setNote("Every server needs a name and a command.");
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
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-4">
      <div className="flex max-h-[85vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-neutral-800 bg-neutral-950">
        <div className="flex items-center justify-between border-b border-neutral-800 px-5 py-3">
          <div>
            <h2 className="text-sm font-semibold">MCP servers</h2>
            <p className="text-[11px] text-neutral-500">
              {status
                ? `${status.runningCount} running · ${status.toolCount} tool${status.toolCount === 1 ? "" : "s"} offered to the model`
                : "Loading…"}
            </p>
          </div>
          <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200">
            ×
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-2 border-b border-neutral-800 px-5 py-2">
          <button
            onClick={save}
            disabled={busy || !dirty}
            className="rounded-lg bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
          >
            Save &amp; restart all
          </button>
          <button
            onClick={reload}
            disabled={busy}
            className="rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 hover:border-neutral-700 disabled:opacity-40"
          >
            Re-read file
          </button>
          <button
            onClick={() => {
              setServers((prev) => [...prev, blankServer()]);
              setDirty(true);
            }}
            className="rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 hover:border-neutral-700"
          >
            + Add server
          </button>
          <span className="ml-auto text-[11px] text-neutral-600">{note}</span>
        </div>

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
                    className="shrink-0 rounded-md border border-neutral-800 px-2 py-1 text-[11px] text-neutral-400 hover:border-neutral-700 disabled:opacity-40"
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
                    ×
                  </button>
                </div>

                <div className="flex gap-2">
                  <input
                    className={`${smallCls} flex-1 font-mono`}
                    value={server.command}
                    onChange={(e) => patch(index, { command: e.target.value })}
                    placeholder="command, e.g. npx or uvx"
                    spellCheck={false}
                  />
                  <input
                    className={`${smallCls} flex-[2] font-mono`}
                    value={server.args.join(" ")}
                    onChange={(e) =>
                      patch(index, { args: e.target.value.split(/\s+/).filter(Boolean) })
                    }
                    placeholder="arguments"
                    spellCheck={false}
                  />
                </div>

                <details className="text-[11px] text-neutral-500">
                  <summary className="cursor-pointer select-none">Environment</summary>
                  <textarea
                    className={`${smallCls} mt-1 h-16 resize-y font-mono`}
                    value={envToText(server.env)}
                    onChange={(e) => patch(index, { env: parseEnv(e.target.value) })}
                    placeholder="API_TOKEN=…&#10;one KEY=value per line"
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
      </div>
    </div>
  );
}
