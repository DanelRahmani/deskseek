import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./index.css";
import App from "./App";
import { QuickEntry } from "./components/QuickEntry";
import { ErrorBoundary } from "./components/ErrorBoundary";

/**
 * Two windows render this one bundle: the app, and the small always-on-top window
 * the chord raises. The label is what tells them apart — a second HTML entry
 * point would be a second copy of the build for one text box.
 *
 * `main` is the fallback rather than the exception, because opening the dev
 * server in an ordinary browser is a thing people do while working on the UI.
 */
function windowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "main";
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {/* Outside App so a throw anywhere in it lands on a readable screen. */}
    <ErrorBoundary>
      {windowLabel() === "quick" ? <QuickEntry /> : <App />}
    </ErrorBoundary>
  </React.StrictMode>,
);
