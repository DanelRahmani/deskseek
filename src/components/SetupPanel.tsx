import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import type { EnvProbe, Settings } from "../types";

interface Props {
  settings: Settings;
  onOpenSettings: () => void;
  onSaved: (settings: Settings) => void;
  onClose: () => void;
}

/// Shown on first run (and again via `/setup`). Two of the three things
/// DeskSeek can do well need something installed that we cannot install for
/// you, so the honest move is to ask rather than quietly leave them broken.
const SEARXNG_COMPOSE = `docker run -d --name searxng -p 8888:8080 \\
  -v searxng:/etc/searxng searxng/searxng`;

export function SetupPanel({ settings, onOpenSettings, onSaved, onClose }: Props) {
  const [probe, setProbe] = useState<EnvProbe | null>(null);
  const [url, setUrl] = useState(settings.searxngUrl);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const refresh = async () => {
    try {
      setProbe(await api.probeEnvironment());
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    refresh();
    // Probe once on open; re-checking is a button, not a loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const saveSearch = async () => {
    setBusy(true);
    setError(null);
    try {
      onSaved(await api.saveSettings({ ...settings, searxngUrl: url.trim() }));
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const finish = async () => {
    setBusy(true);
    setError(null);
    try {
      onSaved(await api.saveSettings({ ...settings, setupDone: true }));
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const keyOk = probe?.hasApiKey ?? false;
  const searchOk = probe?.searxngOk ?? false;
  const sandboxOk = probe?.docker ?? probe?.wsl ?? false;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4">
      <div className="max-h-full w-full max-w-xl overflow-y-auto rounded-2xl border border-neutral-800 bg-neutral-950 p-5 shadow-xl">
        <h1 className="text-base font-semibold text-neutral-100">Set up DeskSeek</h1>
        <p className="mt-1 text-xs text-neutral-500">
          Three things make this app work well. Only one of them is required.
        </p>

        {error && (
          <div className="mt-3 rounded-lg border border-red-900/60 bg-red-950/40 px-3 py-2 text-xs text-red-300">
            {error}
          </div>
        )}

        <Section
          title="Model access"
          required
          done={keyOk}
          doneLabel="API key set"
          todoLabel="No API key yet"
          body="DeskSeek talks to the DeepSeek API, so it needs a key. It is stored in the OS keychain, never in a config file."
          action={
            <button onClick={onOpenSettings} className={button}>
              {keyOk ? "Change key" : "Add your key"}
            </button>
          }
        />

        <Section
          title="Web search"
          done={searchOk}
          doneLabel="SearXNG answering"
          todoLabel="Not configured"
          body={
            <>
              The <code className="text-neutral-400">web_search</code> tool queries a SearXNG
              instance — DeskSeek does not ship one and does not use a paid search API. Without
              it, the other two browsing tools still work. Self-host it, or point this at an
              instance you trust:
              <pre className="mt-2 overflow-x-auto rounded-lg border border-neutral-800 bg-neutral-900 p-2 text-[11px] text-neutral-400">
                {SEARXNG_COMPOSE}
              </pre>
              <span className="mt-1 block text-[11px] text-neutral-600">
                A fresh instance only speaks JSON once you add <code>json</code> to the{" "}
                <code>formats</code> list in its <code>settings.yml</code>.
              </span>
            </>
          }
          action={
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="http://localhost:8888"
                className="min-w-40 flex-1 rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-none focus:border-neutral-700"
              />
              <button onClick={saveSearch} disabled={busy} className={button}>
                Test &amp; save
              </button>
              <button
                onClick={() => {
                  navigator.clipboard?.writeText(SEARXNG_COMPOSE);
                  setCopied(true);
                }}
                className={quiet}
              >
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
          }
        />

        <Section
          title="Sandboxed execution"
          done={sandboxOk}
          doneLabel={probe?.docker ? "Docker found" : "WSL2 found"}
          todoLabel="Neither found"
          body={
            <>
              Coding tasks run commands on your machine. With Docker (or WSL2) they can run
              inside a container instead, which is the difference between a guardrail and real
              isolation. DeskSeek never installs either one for you — it only uses what is
              already there.
            </>
          }
          action={
            <div className="flex flex-wrap items-center gap-2">
              <button
                onClick={() => openUrl("https://docs.docker.com/desktop/install/windows-install/")}
                className={quiet}
              >
                Docker install guide
              </button>
              <button
                onClick={() =>
                  openUrl("https://learn.microsoft.com/windows/wsl/install")
                }
                className={quiet}
              >
                WSL2 install guide
              </button>
            </div>
          }
        />

        <div className="mt-5 flex items-center justify-between gap-2 border-t border-neutral-800 pt-4">
          <button onClick={refresh} className={quiet} disabled={busy}>
            Re-check
          </button>
          <div className="flex items-center gap-2">
            <button onClick={finish} className={quiet} disabled={busy}>
              Skip for now
            </button>
            <button onClick={finish} className={primary} disabled={busy}>
              Finish setup
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

const button =
  "rounded-md border border-neutral-800 px-2 py-1 text-xs text-neutral-300 transition hover:border-neutral-700 hover:text-neutral-100";
const quiet =
  "rounded-md px-2 py-1 text-xs text-neutral-500 transition hover:text-neutral-300 disabled:opacity-50";
const primary =
  "rounded-md bg-sky-600 px-3 py-1 text-xs font-medium text-white transition hover:bg-sky-500 disabled:opacity-50";

function Section({
  title,
  body,
  action,
  done,
  doneLabel,
  todoLabel,
  required,
}: {
  title: string;
  body: ReactNode;
  action: ReactNode;
  done: boolean;
  doneLabel: string;
  todoLabel: string;
  required?: boolean;
}) {
  return (
    <div className="mt-4 rounded-xl border border-neutral-800 bg-neutral-900/40 p-3">
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium text-neutral-200">{title}</span>
        {required && (
          <span className="rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-400">
            required
          </span>
        )}
        <span
          className={`ml-auto rounded px-1.5 py-0.5 text-[10px] ${
            done ? "bg-emerald-500/10 text-emerald-400" : "bg-amber-500/10 text-amber-400"
          }`}
        >
          {done ? `✓ ${doneLabel}` : todoLabel}
        </span>
      </div>
      <p className="mt-2 text-xs leading-relaxed text-neutral-500">{body}</p>
      <div className="mt-3">{action}</div>
    </div>
  );
}
