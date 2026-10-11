import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { copyText } from "../lib/clipboard";
import { IconButton } from "./Icon";

/** The rehype plugin's type, without importing it for real (type queries erase). */
type Highlighter = typeof import("rehype-highlight").default;

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

/**
 * Copy, with a tick for a second so a click is never silent.
 *
 * The icon carries the meaning and the tooltip carries the words: a bordered
 * box saying "Copy" is heavier than the thing it copies.
 */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const [done, setDone] = useState(false);
  return (
    <IconButton
      icon={done ? "check" : "copy"}
      label={done ? "Copied" : label}
      tone={done ? "ok" : "plain"}
      onClick={() => {
        void copyText(text);
        setDone(true);
        window.setTimeout(() => setDone(false), 1200);
      }}
    />
  );
}

export function Markdown({ children }: { children: string }) {
  // highlight.js is the largest thing in the bundle (~600 kB, 127 kB gzipped) and
  // most answers contain no code. It is fetched on the first message that has a
  // fenced block, so a window of prose never pays for it. The cost is one frame
  // of unhighlighted code on that first block, which is worth the load.
  const [highlight, setHighlight] = useState<Highlighter | null>(null);
  const wantsCode = children.includes("```") || children.includes("~~~");
  useEffect(() => {
    if (!wantsCode || highlight) return;
    let alive = true;
    // `() => m.default` rather than passing the function to setState directly:
    // React would call a bare function argument as an updater.
    void import("rehype-highlight").then((m) => {
      if (alive) setHighlight(() => m.default);
    });
    return () => {
      alive = false;
    };
  }, [wantsCode, highlight]);

  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={highlight ? [highlight] : []}
        components={{
          // Every fenced block gets its own copy button: taking a snippet out of
          // an answer is the most common thing anyone does with one. It sits on
          // the wrapper rather than inside the `pre`, so scrolling a long line
          // sideways does not carry the button off the edge.
          pre({ children }) {
            return (
              <div className="group relative">
                <div className="absolute top-1.5 right-1.5 z-10 opacity-55 transition duration-150 group-hover:opacity-100 focus-within:opacity-100">
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
