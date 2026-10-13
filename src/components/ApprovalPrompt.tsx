import type { ApprovalEvent } from "../types";

interface Props {
  request: ApprovalEvent;
  onDecide: (requestId: string, decision: "once" | "session" | "always" | "deny") => void;
}

function Diff({ before, after }: { before: string; after: string }) {
  return (
    <pre className="mt-2 max-h-64 overflow-auto rounded-md border border-neutral-800 bg-neutral-950 p-2 text-[11px] leading-relaxed">
      {before.split("\n").map((line, i) => (
        <div key={`old-${i}`} className="text-red-300/90">
          - {line}
        </div>
      ))}
      {after.split("\n").map((line, i) => (
        <div key={`new-${i}`} className="text-emerald-300/90">
          + {line}
        </div>
      ))}
    </pre>
  );
}

function detail(request: ApprovalEvent) {
  const args = request.args ?? {};
  const text = (key: string) =>
    typeof args[key] === "string" ? (args[key] as string) : undefined;

  if (request.tool === "run_command") {
    return (
      <pre className="mt-2 overflow-x-auto rounded-md border border-neutral-800 bg-neutral-950 p-2 text-[11px] text-amber-200">
        {text("command")}
      </pre>
    );
  }
  if (request.tool === "edit_file") {
    return (
      <>
        <p className="mt-2 font-mono text-[11px] text-neutral-400">{text("path")}</p>
        <Diff before={text("old_string") ?? ""} after={text("new_string") ?? ""} />
      </>
    );
  }
  if (request.tool === "write_file") {
    const content = text("content") ?? "";
    const lines = content.split("\n");
    const shown = lines.slice(0, 40).join("\n");
    return (
      <>
        <p className="mt-2 font-mono text-[11px] text-neutral-400">{text("path")}</p>
        <pre className="mt-2 max-h-64 overflow-auto rounded-md border border-neutral-800 bg-neutral-950 p-2 text-[11px] text-emerald-300/90">
          {shown}
          {lines.length > 40 ? `\n… ${lines.length - 40} more lines` : ""}
        </pre>
      </>
    );
  }
  if (request.tool === "git_commit") {
    return (
      <p className="mt-2 text-[11px] text-neutral-400">
        Commit message: <span className="text-neutral-200">{text("message")}</span>
      </p>
    );
  }
  return (
    <pre className="mt-2 overflow-x-auto rounded-md border border-neutral-800 bg-neutral-950 p-2 text-[11px] text-neutral-400">
      {JSON.stringify(args, null, 2)}
    </pre>
  );
}

export function ApprovalPrompt({ request, onDecide }: Props) {
  const label =
    request.class === "write"
      ? "wants to change a file"
      : "wants to run something in your workspace";

  return (
    <div className="mb-3 rounded-xl border border-amber-900/60 bg-amber-950/20 p-3">
      <div className="flex items-center gap-2 text-xs text-amber-200">
        <span>⚠</span>
        <span className="font-medium">
          DeskSeek {label}: <span className="font-mono">{request.tool}</span>
        </span>
      </div>
      <p className="mt-1 text-[11px] text-neutral-500">
        in <span className="font-mono">{request.workspace}</span>
      </p>
      {detail(request)}
      <div className="mt-3 flex flex-wrap gap-2">
        <button
          onClick={() => onDecide(request.requestId, "once")}
          className="rounded-md bg-amber-500 px-3 py-1.5 text-xs font-medium text-black hover:bg-amber-400"
        >
          Allow once
        </button>
        <button
          onClick={() => onDecide(request.requestId, "session")}
          className="rounded-md border border-neutral-700 px-3 py-1.5 text-xs text-neutral-200 hover:border-neutral-600"
        >
          Allow for this session
        </button>
        <button
          onClick={() => onDecide(request.requestId, "always")}
          className="rounded-md border border-neutral-700 px-3 py-1.5 text-xs text-neutral-200 hover:border-neutral-600"
          title={
            request.target
              ? `Permanently allow ${request.tool} for ${request.target}, in this folder`
              : `Permanently allow ${request.tool}, wherever it acts, in this folder`
          }
        >
          {request.target
            ? `Always allow ${request.target} for this folder`
            : "Always allow for this folder"}
        </button>
        <button
          onClick={() => onDecide(request.requestId, "deny")}
          className="rounded-md border border-red-900/70 px-3 py-1.5 text-xs text-red-300 hover:border-red-800"
        >
          Deny
        </button>
      </div>
    </div>
  );
}
