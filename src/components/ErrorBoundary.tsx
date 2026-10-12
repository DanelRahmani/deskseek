import { Component, type ErrorInfo, type ReactNode } from "react";
import { copyText } from "../lib/clipboard";
import { IconButton } from "./Icon";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  /** React's component stack, which names what threw. */
  where: string;
}

/**
 * The last resort in the window.
 *
 * React unmounts everything under a component that throws, so one bad render
 * used to leave a blank window and nothing to go on — the bug had no way to
 * report itself, and the user had no way to describe it. Catching it here costs
 * one class component and turns that into a readable screen: the message, what
 * threw, and a way back.
 *
 * ponytail: it does not preserve in-progress state, because a half-typed prompt
 * lives in the tree it replaces. *Try again* re-renders the app, which is enough
 * to recover from a bad payload that has since changed; keeping the draft means
 * lifting it above the boundary, which is worth doing only if this recurs.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, where: "" };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // The console is where a developer looks; the screen is where the user does.
    console.error("DeskSeek hit a render error:", error, info.componentStack);
    this.setState({ where: info.componentStack ?? "" });
  }

  render() {
    const { error, where } = this.state;
    if (!error) return this.props.children;

    const report = `${error.name}: ${error.message}\n${where}`.trim();
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-4 bg-neutral-950 p-8">
        <h1 className="text-sm font-semibold text-neutral-100">Something in the window broke</h1>
        <p className="max-w-md text-center text-[13px] leading-relaxed text-neutral-500">
          This is a bug in the app, not something you did. The message below says what failed —
          copying it into a new chat is the quickest way to get it looked at.
        </p>
        <pre className="max-h-64 w-full max-w-2xl overflow-auto rounded-lg border border-neutral-800 bg-neutral-900/60 p-3 font-mono text-[11px] leading-relaxed text-red-300/90">
          {report}
        </pre>
        <div className="flex items-center gap-2">
          <button
            onClick={() => this.setState({ error: null, where: "" })}
            className="rounded-lg bg-sky-700 px-3 py-1.5 text-[13px] text-accent-fg transition duration-150 hover:bg-sky-600"
          >
            Try again
          </button>
          <button
            onClick={() => window.location.reload()}
            className="rounded-lg px-3 py-1.5 text-[13px] text-neutral-400 transition duration-150 hover:bg-neutral-800 hover:text-neutral-100"
          >
            Reload the window
          </button>
          <IconButton
            icon="copy"
            label="Copy the error"
            onClick={() => void copyText(report)}
          />
        </div>
      </div>
    );
  }
}
