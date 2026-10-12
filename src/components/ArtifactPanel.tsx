import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import { frameDoc, KIND_LABELS, SANDBOX } from "../lib/artifact";
import { chartSvg } from "../lib/chart";
import type { ArtifactDetail } from "../types";
import { Markdown } from "./Markdown";

interface Props {
  detail: ArtifactDetail;
  onClose: () => void;
  /** A new version was saved; the app re-mounts the panel with it. */
  onChange: (detail: ArtifactDetail) => void;
}

/**
 * The artifact side panel: the document, a version picker, and an editor. A
 * saved edit goes back through the app so the panel re-mounts on the new
 * version rather than holding state that is now stale.
 */
export function ArtifactPanel({ detail, onClose, onChange }: Props) {
  const { artifact } = detail;
  const [version, setVersion] = useState(artifact.current_version);
  const [content, setContent] = useState(detail.content);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(detail.content);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isCurrent = version === artifact.current_version;

  /** The panel is a reading surface, so the common next move is to take the
   *  document somewhere else. Copy the version on screen, not the current one. */
  async function copy() {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setError("Couldn't reach the clipboard.");
    }
  }

  async function selectVersion(next: number) {
    setEditing(false);
    if (next === artifact.current_version) {
      setContent(detail.content);
      setVersion(next);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const older = await api.getArtifact(artifact.id, next);
      setContent(older.content);
      setVersion(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      onChange(await api.saveArtifactVersion(artifact.id, draft));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** Restoring is an edit: the old content becomes a new version, so nothing
   *  is lost either way. */
  async function restore() {
    setBusy(true);
    setError(null);
    try {
      onChange(await api.saveArtifactVersion(artifact.id, content));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <aside className="flex h-full w-[480px] shrink-0 flex-col border-l border-neutral-800 bg-neutral-950">
      <header className="flex shrink-0 items-center gap-2 border-b border-neutral-800 px-3 py-2">
        <span className="truncate text-xs font-medium text-neutral-200" title={artifact.title}>
          {artifact.title}
        </span>
        <span className="shrink-0 rounded bg-neutral-800 px-1.5 py-0.5 text-[10px] text-neutral-400">
          {KIND_LABELS[artifact.kind] ?? artifact.kind}
        </span>
        <button
          onClick={onClose}
          className="ml-auto text-sm text-neutral-500 transition hover:text-neutral-200"
          title="Close the artifact"
        >
          <Icon name="close" size={15} />
        </button>
      </header>

      <div className="flex shrink-0 items-center gap-2 border-b border-neutral-800 px-3 py-1.5">
        <select
          value={version}
          onChange={(e) => selectVersion(Number(e.target.value))}
          className="rounded border border-neutral-700 bg-neutral-900 px-1.5 py-0.5 text-[11px] text-neutral-300"
          title="Version"
        >
          {detail.versions.map((v) => (
            <option key={v.version} value={v.version}>
              v{v.version} · {v.created_by === "user" ? "you" : "the model"}
            </option>
          ))}
        </select>

        {!isCurrent && (
          <button
            onClick={restore}
            disabled={busy}
            className="rounded border border-neutral-700 px-2 py-0.5 text-[11px] text-neutral-300 transition hover:bg-neutral-800 disabled:opacity-50"
            title="Make this the current version again"
          >
            Restore
          </button>
        )}

        <div className="ml-auto flex items-center gap-2">
          {editing ? (
            <>
              <button
                onClick={save}
                disabled={busy}
                className="rounded bg-sky-700 px-2 py-0.5 text-[11px] text-accent-fg transition hover:bg-sky-600 disabled:opacity-50"
              >
                {busy ? "Saving…" : "Save"}
              </button>
              <button
                onClick={() => {
                  setEditing(false);
                  setDraft(content);
                }}
                className="rounded px-2 py-0.5 text-[11px] text-neutral-400 transition hover:text-neutral-200"
              >
                Cancel
              </button>
            </>
          ) : (
            <button
              onClick={() => setEditing(true)}
              className="rounded border border-neutral-700 px-2 py-0.5 text-[11px] text-neutral-300 transition hover:bg-neutral-800"
            >
              Edit
            </button>
          )}
          {!editing && (
            <button
              onClick={copy}
              className="rounded border border-neutral-700 px-2 py-0.5 text-[11px] text-neutral-300 transition hover:bg-neutral-800"
              title="Copy this version to the clipboard"
            >
              {copied ? "Copied" : "Copy"}
            </button>
          )}
        </div>
      </div>

      {error && (
        <p className="shrink-0 whitespace-pre-wrap border-b border-amber-900/50 bg-amber-950/40 px-3 py-1.5 text-[11px] text-amber-300">
          {error}
        </p>
      )}

      {editing ? (
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          spellCheck={false}
          className="min-h-0 flex-1 resize-none bg-neutral-950 p-3 font-mono text-xs leading-relaxed text-neutral-200 outline-none"
        />
      ) : (
        <div className="min-h-0 flex-1 overflow-hidden">
          <View kind={artifact.kind} content={content} title={artifact.title} />
        </div>
      )}
    </aside>
  );
}

function View({ kind, content, title }: { kind: string; content: string; title: string }) {
  if (kind === "markdown") {
    return (
      <div className="h-full overflow-auto p-4">
        <Markdown>{content}</Markdown>
      </div>
    );
  }
  if (kind === "code") {
    // ponytail: plain <pre>, no editor dependency. Add CodeMirror when editing
    // code with highlighting is worth ~400 kB; a textarea is already the editor.
    return (
      <pre className="h-full overflow-auto whitespace-pre p-3 font-mono text-xs leading-relaxed text-neutral-200">
        {content}
      </pre>
    );
  }
  if (kind === "mermaid") return <Mermaid source={content} />;

  // A chart is a spec, not markup: `chartSvg` builds the picture. Everything
  // else reaches the frame as the document the model wrote.
  let doc = frameDoc(kind === "svg" ? "svg" : "html", content);
  if (kind === "chart") {
    const drawn = chartSvg(content);
    if ("error" in drawn) return <CouldNotDraw what="chart" error={drawn.error} source={content} />;
    doc = frameDoc("svg", drawn.svg);
  }
  return (
    <iframe
      // The frame is the sandbox: unique origin, no network, its own scripts.
      sandbox={SANDBOX}
      srcDoc={doc}
      className="h-full w-full border-0 bg-white"
      title={title}
    />
  );
}

/**
 * Something the panel was asked to draw and could not. The reason comes first
 * and the source underneath, because the source is the part the user — or the
 * model, on the next turn — can actually fix.
 */
function CouldNotDraw({ what, error, source }: { what: string; error: string; source: string }) {
  return (
    <div className="h-full overflow-auto p-3">
      <p className="mb-2 text-[11px] text-amber-400">That {what} could not be drawn: {error}</p>
      <pre className="whitespace-pre font-mono text-xs text-neutral-300">{source}</pre>
    </div>
  );
}

/** Mermaid is a big chunk, so it is fetched only when a diagram is on screen. */
function Mermaid({ source }: { source: string }) {
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: "neutral" });
        const drawn = await mermaid.render(`artifact-${Date.now()}`, source);
        if (live) setSvg(drawn.svg);
      } catch (e) {
        if (live) setError(String(e));
      }
    })();
    return () => {
      live = false;
    };
  }, [source]);

  if (error) {
    return <CouldNotDraw what="diagram" error={error} source={source} />;
  }
  if (!svg) return <p className="p-4 text-xs text-neutral-500">Drawing the diagram…</p>;
  return (
    <iframe
      sandbox={SANDBOX}
      srcDoc={frameDoc("mermaid", svg)}
      className="h-full w-full border-0 bg-white"
      title="Diagram"
    />
  );
}
