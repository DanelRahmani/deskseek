import { useState } from "react";

import { api } from "../api";
import type { EnvProbe } from "../types";

interface Props {
  probe: EnvProbe | null;
  /** The URL it came up on. The caller re-reads the settings the Rust side wrote. */
  onDone: (url: string) => void | Promise<void>;
}

const button =
  "rounded-md border border-neutral-800 px-2 py-1 text-xs text-neutral-300 transition hover:border-neutral-700 hover:text-neutral-100 disabled:opacity-50";
const quiet =
  "rounded-md px-2 py-1 text-xs text-neutral-500 transition hover:text-neutral-300 disabled:opacity-50";

/**
 * Bringing a local SearXNG instance up, as one control.
 *
 * One copy rather than two, because it is the same three steps either way —
 * start the container, re-probe, hand the URL back — and the two screens that
 * need it only disagree about what happens when it works.
 *
 * The second caller is the point: the setup screen is where somebody is on their
 * first run, but a person who uses `web_search` and finds it broken looks in
 * **Settings → Browsing**, where the URL field already lives. A button only on the
 * first-run screen is a button half the users cannot find.
 *
 * What it deliberately does not do is decide whether the instance is *wanted* —
 * `probe.searxngOk` hides it once one is answering, and the caller decides that.
 */
export function SearxngInstall({ probe, onDone }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Docker's engine answers tens of seconds after its launcher returns, so
  // "started" is worth saying rather than leaving a button that looks inert.
  const [started, setStarted] = useState(false);

  const install = async () => {
    setBusy(true);
    setError(null);
    try {
      await onDone(await api.installSearxng());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const startDocker = async () => {
    setError(null);
    try {
      await api.startDocker();
      setStarted(true);
    } catch (e) {
      setError(String(e));
    }
  };

  // Why the button is or is not available, in the order the user meets the
  // problems: no Docker at all, Docker installed but asleep, then ready.
  const hint = probe?.dockerRunning
    ? "Builds the container, writes the config it needs, and waits until it answers."
    : probe?.dockerCli
      ? "Docker is installed but its engine is not answering."
      : "Needs Docker — the same engine the command sandbox uses.";

  return (
    <div className="flex flex-col items-start gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <button onClick={install} disabled={busy || !probe?.dockerRunning} className={button}>
          {busy ? "Setting it up…" : "Set it up for me"}
        </button>
        {probe?.dockerCli && !probe?.dockerRunning && (
          <button onClick={startDocker} className={quiet}>
            Start Docker Desktop
          </button>
        )}
        <span className="text-[11px] text-neutral-500">
          {started && !probe?.dockerRunning
            ? "Docker Desktop is starting — it takes a few seconds to answer, then press Re-check."
            : hint}
        </span>
      </div>
      {error && (
        <p role="alert" className="text-[11px] text-red-400">
          {error}
        </p>
      )}
    </div>
  );
}
