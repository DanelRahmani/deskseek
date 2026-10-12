/**
 * Folding the flat transcript into messages and runs of tool activity.
 *
 * A tool-using turn is stored as several rows: an assistant row carrying
 * `tool_calls_json`, one `tool` row per call, then the next round. Rendered one
 * by one that is a column of near-identical cards — read twelve files and the
 * turn is twelve boxes. This module groups a run of them into one item so the
 * transcript shows "12 steps" until the user opens it.
 *
 * Kept pure and outside the component so the pairing (which tool a result
 * belongs to, and whether a live run is the same one already stored) can be
 * checked without a browser — see transcript.check.ts.
 */
import type { MessageRow, ToolRun } from "../types";

export type TranscriptItem =
  | { kind: "message"; message: MessageRow }
  | { kind: "steps"; steps: ToolRun[] };

/** One entry of a stored assistant `tool_calls_json` array. */
interface StoredCall {
  id?: string;
  function?: { name?: string; arguments?: string };
}

function parseCalls(json: string): StoredCall[] {
  try {
    const value: unknown = JSON.parse(json);
    return Array.isArray(value) ? (value as StoredCall[]) : [];
  } catch {
    return [];
  }
}

function parseArgs(raw: string | undefined): unknown {
  if (!raw) return undefined;
  try {
    return JSON.parse(raw);
  } catch {
    return undefined;
  }
}

/** An assistant row whose only job is to carry tool calls — no prose. */
function isToolTurn(m: MessageRow): boolean {
  return m.role === "assistant" && !!m.tool_calls_json && !(m.content ?? "").trim();
}

/** The status a stored result implies. Failures are written as `Error: …`. */
function storedStatus(content: string | null): ToolRun["status"] {
  return content?.startsWith("Error:") ? "error" : "ok";
}

/**
 * Group the stored transcript into messages and step runs.
 *
 * Steps are filled in as the matching `tool` rows arrive, so a group is complete
 * by the time the turn is stored. A result whose call row is missing still shows
 * as its own step, so nothing is hidden by grouping.
 */
function fromMessages(messages: MessageRow[]): TranscriptItem[] {
  const items: TranscriptItem[] = [];
  let steps: ToolRun[] | null = null;
  const flush = () => {
    if (steps && steps.length) items.push({ kind: "steps", steps });
    steps = null;
  };

  for (const message of messages) {
    if (isToolTurn(message)) {
      steps = steps ?? [];
      for (const call of parseCalls(message.tool_calls_json ?? "")) {
        steps.push({
          id: call.id ?? `call-${steps.length}`,
          tool: call.function?.name ?? "tool",
          args: parseArgs(call.function?.arguments),
          status: "ok",
        });
      }
      continue;
    }

    if (message.role === "tool") {
      steps = steps ?? [];
      const status = storedStatus(message.content);
      const at = message.tool_call_id
        ? steps.findIndex((s) => s.id === message.tool_call_id)
        : -1;
      if (at >= 0) {
        steps[at] = { ...steps[at], status, output: message.content ?? undefined };
      } else {
        steps.push({
          id: message.tool_call_id ?? message.id,
          tool: "tool",
          args: undefined,
          status,
          output: message.content ?? undefined,
        });
      }
      continue;
    }

    flush();
    items.push({ kind: "message", message });
  }

  flush();
  return items;
}

/**
 * Fold in the turn in flight.
 *
 * The live `ToolRun`s carry the real status and any screenshot, and the stored
 * rows for the same calls appear the moment the turn is saved — so a run already
 * in the transcript is updated in place rather than drawn a second time.
 */
function mergeLive(items: TranscriptItem[], tools: ToolRun[]): void {
  const byId = new Map<string, ToolRun>();
  for (const item of items) {
    if (item.kind === "steps") for (const step of item.steps) byId.set(step.id, step);
  }

  const trailing = () => {
    const last = items[items.length - 1];
    if (last && last.kind === "steps") return last.steps;
    const steps: ToolRun[] = [];
    items.push({ kind: "steps", steps });
    return steps;
  };

  for (const run of tools) {
    const known = byId.get(run.id);
    if (known) {
      Object.assign(known, run);
    } else {
      const steps = trailing();
      steps.push(run);
      byId.set(run.id, run);
    }
  }
}

/** The transcript as the view should draw it: messages and grouped steps. */
export function buildTranscript(messages: MessageRow[], tools: ToolRun[]): TranscriptItem[] {
  const items = fromMessages(messages);
  if (tools.length) mergeLive(items, tools);
  return items;
}
