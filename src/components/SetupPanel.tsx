import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { SearxngInstall } from "./SearxngInstall";
import type { EnvProbe, SandboxMode, Settings } from "../types";

interface Props {
  settings: Settings;
  onOpenSettings: () => void;
  onSaved: (settings: Settings) => void;
  onClose: () => void;
}

/** How a setup item reads: settled, needs a nudge, or simply not chosen. */
type State = "ok" | "warn" | "todo";

/// Shown on first run (and again via `/setup`). Two of the three things
/// DeskSeek can do well need something installed that we cannot install for
/// you, so the honest move is to ask rather than quietly leave them broken.
const SEARXNG_COMPOSE = `docker run -d --name searxng -p 127.0.0.1:8888:8080 \\
  -v searxng:/etc/searxng searxng/searxng`;

/// Ordered from least to most isolating, which is also the order they are
/// explained in. "Auto" is first because it is the default.
export const SANDBOX_MODES: { id: SandboxMode; label: string; hint: string }[] = [
  { id: "auto", label: "Auto", hint: "Use the strongest rung this machine can reach" },
  { id: "host", label: "This machine", hint: "Run directly, with the guard as the only protection" },
  { id: "docker", label: "Docker", hint: "Run in a throwaway container with no network" },
  { id: "wsl", label: "WSL2", hint: "Run inside a WSL distribution" },
];

export const RUNG_LABEL: Record<string, string> = {
  docker: "a Docker container",
  wsl: "a WSL distribution",
  host: "this machine",
};

export function SetupPanel({ settings, onOpenSettings, onSaved, onClose }: Props) {
  const [probe, setProbe] = useState<EnvProbe | null>(null);
  const [url, setUrl] = useState(settings.searxngUrl);
  const [image, setImage] = useState(settings.sandboxImage);
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

  // Called once the container is up. The URL was written on the Rust side, so read
  // the settings back rather than assuming what they now say.
  const afterInstall = async (url: string) => {
    setUrl(url);
    onSaved(await api.getSettings());
    await refresh();
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

  // "Installed" and "running" are different answers, and the second one is the
  // one that matters — a stopped Docker Desktop is not a missing Docker.
  const capability: { state: State; label: string } = probe?.dockerRunning
    ? { state: "ok", label: "Docker is running" }
    : probe?.wsl
      ? { state: "ok", label: "WSL2 found" }
      : probe?.dockerCli
        ? { state: "warn", label: "Docker installed, engine stopped" }
        : { state: "todo", label: "Neither found" };

  const rung = probe?.sandboxRung ?? "host";

  const save = async (patch: Partial<Settings>) => {
    setError(null);
    try {
      onSaved(await api.saveSettings({ ...settings, ...patch }));
      // The rung is computed from the mode, so ask again rather than guess.
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Set up DeskSeek"
        className="max-h-full w-full max-w-xl overflow-y-auto rounded-2xl border border-neutral-800 bg-neutral-950 p-5 shadow-xl"
      >
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
          state={keyOk ? "ok" : "todo"}
          stateLabel={keyOk ? "API key set" : "No API key yet"}
          body="DeskSeek talks to the DeepSeek API, so it needs a key. It is stored in the OS keychain, never in a config file."
          action={
            <button onClick={onOpenSettings} className={button}>
              {keyOk ? "Change key" : "Add your key"}
            </button>
          }
        />

        <Section
          title="Web search"
          state={searchOk ? "ok" : "todo"}
          stateLabel={searchOk ? "SearXNG answering" : "Not configured"}
          body={
            <>
              The <code className="text-neutral-400">web_search</code> tool queries a SearXNG
              instance — DeskSeek does not ship one and does not use a paid search API. Without
              it, the other two browsing tools still work. This screen can set one up for you on
              Docker, or you can point it at an instance you already trust:
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
            <div className="flex flex-col items-start gap-2">
              {!searchOk && <SearxngInstall probe={probe} onDone={afterInstall} />}
              <div className="flex flex-wrap items-center gap-2">
                <input
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="http://localhost:8888"
                  className="min-w-40 flex-1 rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-hidden focus:border-neutral-700"
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
            </div>
          }
        />

        <Section
          title="Where commands run"
          state={capability.state}
          stateLabel={capability.label}
          body={
            <>
              Shell commands can run on your machine, or inside a Docker container or a WSL
              distribution instead. The second and third are real isolation: they cannot see
              the rest of your computer. A container does, however, only have the tools its
              image was built with, and it gets no network. DeskSeek never installs either one
              for you — it only uses what is already there.
              <span className="mt-2 block text-neutral-400">
                Commands currently run in{" "}
                <strong className="text-neutral-200">{RUNG_LABEL[rung]}</strong>.
              </span>
              {probe?.sandboxNote && (
                <span className="mt-1 block text-amber-400/80">{probe.sandboxNote}</span>
              )}
              {capability.state === "warn" && (
                <span className="mt-2 block text-amber-400/80">
                  Docker is installed but its engine is not answering. Start Docker Desktop,
                  then press Re-check.
                </span>
              )}
              <span className="mt-2 block text-[11px] text-neutral-600">
                Choosing “This machine” leaves you with the workspace sandbox, the scrubbed
                environment and the command guard — a guardrail, not a security boundary.
              </span>
            </>
          }
          action={
            <div className="flex flex-col items-start gap-2">
              <div
                className="flex flex-wrap items-center gap-0.5 rounded-md border border-neutral-800 p-0.5"
                title="Where run_command runs"
              >
                {SANDBOX_MODES.map((mode) => (
                  <button
                    key={mode.id}
                    onClick={() => save({ sandbox: mode.id })}
                    title={mode.hint}
                    className={`rounded px-2 py-0.5 text-[11px] transition ${
                      settings.sandbox === mode.id
                        ? "bg-neutral-800 text-neutral-100"
                        : "text-neutral-500 hover:text-neutral-300"
                    }`}
                  >
                    {mode.label}
                  </button>
                ))}
              </div>
              {rung === "docker" && (
                <input
                  value={image}
                  onChange={(e) => setImage(e.target.value)}
                  onBlur={() => image !== settings.sandboxImage && save({ sandboxImage: image })}
                  placeholder="alpine:latest"
                  spellCheck={false}
                  className="w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-[11px] text-neutral-200 outline-hidden focus:border-neutral-700"
                  title="The image commands run in — it must carry the workspace's toolchain"
                />
              )}
              <div className="flex flex-wrap items-center gap-2">
                <button
                  onClick={() => openUrl("https://docs.docker.com/desktop/install/windows-install/")}
                  className={quiet}
                >
                  Docker install guide
                </button>
                <button
                  onClick={() => openUrl("https://learn.microsoft.com/windows/wsl/install")}
                  className={quiet}
                >
                  WSL2 install guide
                </button>
              </div>
            </div>
          }
        />

        <Section
          title="The user's screen"
          state={settings.computerUse ? "ok" : "todo"}
          stateLabel={settings.computerUse ? "On" : "Off"}
          body={
            <>
              Let the agent read and operate the desktop — the controls of the window that has
              focus, and the names of the others. That is how it answers "what is on my screen",
              reads a dialog you cannot describe, and presses a button when you ask it to.
              <span className="mt-1 block">
                It asks before every press and every keystroke, and{" "}
                <span className="text-neutral-400">Ctrl+Alt+Esc</span> stops it immediately. Off by
                default, because whatever is on screen may be private.
              </span>
            </>
          }
          action={
            <label className="flex cursor-pointer items-start gap-2">
              <input
                type="checkbox"
                checked={settings.computerUse}
                disabled={busy}
                onChange={(e) => save({ computerUse: e.target.checked })}
                className="mt-0.5 accent-sky-500"
              />
              <span className="text-xs text-neutral-400">
                Let the agent read the desktop (Windows only)
              </span>
            </label>
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
  "rounded-md bg-sky-600 px-3 py-1 text-xs font-medium text-accent-fg transition hover:bg-sky-500 disabled:opacity-50";

function Section({
  title,
  body,
  action,
  state,
  stateLabel,
  required,
}: {
  title: string;
  body: ReactNode;
  action: ReactNode;
  state: State;
  stateLabel: string;
  required?: boolean;
}) {
  const tone =
    state === "ok"
      ? "bg-emerald-500/10 text-emerald-400"
      : state === "warn"
        ? "bg-amber-500/10 text-amber-400"
        : "bg-neutral-800 text-neutral-400";

  return (
    <div className="mt-4 rounded-xl border border-neutral-800 bg-neutral-900/40 p-3">
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium text-neutral-200">{title}</span>
        {required && (
          <span className="rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-400">
            required
          </span>
        )}
        <span className={`ml-auto rounded px-1.5 py-0.5 text-[10px] ${tone}`}>
          {state === "ok" ? "✓ " : ""}
          {stateLabel}
        </span>
      </div>
      <p className="mt-2 text-xs leading-relaxed text-neutral-500">{body}</p>
      <div className="mt-3">{action}</div>
    </div>
  );
}
