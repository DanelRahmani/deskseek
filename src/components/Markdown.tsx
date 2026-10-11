import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { copyText } from "../lib/clipboard";

/**
 * The text inside a rendered node, whatever react-markdown nested it in.
 *
 * A `pre` arrives with its `code` child as React elements, not a string, so the
 * raw text has to be walked back out. Anything unexpected yields "" rather than
 * throwing inside a render.
 */
function textOf(node: unknown): string {
  if (node === null || node === undefined || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  const props = (node as { props?: { children?: unknown } }).props;
  return props ? textOf(props.children) : "";
}

/** A copy button that says so for a moment, so a click is never silent. */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      onClick={() => {
        void copyText(text);
        setDone(true);
        window.setTimeout(() => setDone(false), 1200);
      }}
      className="rounded border border-neutral-700 bg-neutral-900/80 px-1.5 py-0.5 text-[10px] text-neutral-400 transition hover:border-neutral-600 hover:text-neutral-200"
      title={label}
    >
      {done ? "Copied" : "Copy"}
    </button>
  );
}

export function Markdown({ children }: { children: string }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          // Every fenced block gets its own copy button: taking a snippet out of
          // an answer is the most common thing anyone does with one.
          pre({ children }) {
            return (
              <div className="relative">
                <div className="absolute top-1 right-1 opacity-60 transition hover:opacity-100">
                  <CopyButton text={textOf(children).replace(/\n$/, "")} label="Copy this block" />
                </div>
                <pre>{children}</pre>
              </div>
            );
          },
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
