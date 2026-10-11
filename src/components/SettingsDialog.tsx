import { useState } from "react";
import { api } from "../api";
import { THEMES } from "../lib/theme";
import type { Instruction, ModelInfo, Settings } from "../types";

interface Props {
  settings: Settings;
  keyMask: string | null;
  models: ModelInfo[];
  documentsDir: string;
  onClose: () => void;
  onSaved: (settings: Settings, keyMask: string | null) => void;
}

const EFFORTS = ["low", "high", "max"];
const SCOPES = ["chat", "coding", "both"];
const inputCls =
  "w-full rounded-lg border border-neutral-800 bg-neutral-900 px-3 py-2 text-sm text-neutral-200 outline-none focus:border-neutral-700";

export function SettingsDialog({
  settings,
  keyMask,
  models,
  documentsDir,
  onClose,
  onSaved,
}: Props) {
  const [form, setForm] = useState<Settings>(settings);
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<"appearance" | "api" | "prompt" | "instructions" | "browsing">(
    "appearance",
  );

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  const setInstruction = (id: string, patch: Partial<Instruction>) =>
    setForm((f) => ({
      ...f,
      instructions: f.instructions.map((i) => (i.id === id ? { ...i, ...patch } : i)),
    }));

  const addInstruction = () =>
    setForm((f) => ({
      ...f,
      instructions: [
        ...f.instructions,
        {
          id: `custom-${Date.now()}`,
          name: "New instruction",
          scope: "chat",
          body: "",
          enabled: true,
        },
      ],
    }));

  const removeInstruction = (id: string) =>
    setForm((f) => ({ ...f, instructions: f.instructions.filter((i) => i.id !== id) }));

  const saveKey = async () => {
    if (!apiKey.trim()) return;
    setBusy(true);
    try {
      await api.setApiKey(apiKey.trim());
      const mask = await api.maskedApiKey();
      setApiKey("");
      setStatus("Key saved to the OS keychain.");
      onSaved(form, mask);
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const clearKey = async () => {
    await api.clearApiKey();
    onSaved(form, null);
    setStatus("Key removed.");
  };

  const testKey = async () => {
    setBusy(true);
    setStatus("Testing…");
    try {
      await api.saveSettings(form);
      await api.testConnection();
      setStatus("Connected — the key works.");
    } catch (e) {
      setStatus(`Failed: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const saveAll = async () => {
    setBusy(true);
    try {
      const saved = await api.saveSettings(form);
      onSaved(saved, keyMask);
      setStatus("Settings saved.");
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  /**
   * Picking a theme saves it straight away rather than waiting for the Save
   * button: a theme you cannot see until you press Save is not a theme picker,
   * and the app repaints from the settings this hands back to it.
   */
  const pickAppearance = async (patch: Partial<Pick<Settings, "theme" | "themeMode">>) => {
    const next = { ...form, ...patch };
    setForm(next);
    try {
      const saved = await api.saveSettings(next);
      onSaved(saved, keyMask);
    } catch (e) {
      setStatus(String(e));
    }
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-4">
      <div className="flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-neutral-800 bg-neutral-950">
        <div className="flex items-center justify-between border-b border-neutral-800 px-5 py-3">
          <h2 className="text-sm font-semibold">Settings</h2>
          <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200">
            ×
          </button>
        </div>

        <div className="flex gap-1 border-b border-neutral-800 px-5 pt-2">
          {(
            [
              ["appearance", "Appearance"],
              ["api", "API"],
              ["prompt", "Prompt & models"],
              ["instructions", "Scoped instructions"],
              ["browsing", "Browsing"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              onClick={() => setTab(id)}
              className={`rounded-t-md px-3 py-2 text-xs font-medium ${
                tab === id
                  ? "border-b-2 border-sky-500 text-neutral-100"
                  : "text-neutral-500 hover:text-neutral-300"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        <div className="flex-1 space-y-5 overflow-y-auto px-5 py-4">
          {tab === "appearance" && (
            <>
              <div>
                <label className="block text-xs text-neutral-500">Theme</label>
                <div className="mt-2 grid grid-cols-3 gap-2">
                  {THEMES.map((theme) => (
                    <button
                      key={theme.id}
                      onClick={() => pickAppearance({ theme: theme.id })}
                      className={`rounded-lg border p-3 text-left text-xs transition ${
                        form.theme === theme.id
                          ? "border-sky-500 text-neutral-100"
                          : "border-neutral-800 text-neutral-400 hover:border-neutral-700"
                      }`}
                    >
                      <span className="mb-2 flex gap-1">
                        {[
                          theme.dark.ramp.n950,
                          theme.dark.ramp.n800,
                          theme.dark.accent,
                          theme.dark.ramp.n200,
                        ].map((colour) => (
                          <span
                            key={colour}
                            className="h-4 w-4 rounded-sm"
                            style={{ background: colour }}
                          />
                        ))}
                      </span>
                      {theme.name}
                    </button>
                  ))}
                </div>
              </div>

              <div>
                <label className="block text-xs text-neutral-500">Mode</label>
                <div className="mt-2 flex gap-1">
                  {(["system", "dark", "light"] as const).map((mode) => (
                    <button
                      key={mode}
                      onClick={() => pickAppearance({ themeMode: mode })}
                      className={`rounded-md border px-3 py-1.5 text-xs capitalize transition ${
                        form.themeMode === mode
                          ? "border-sky-500 text-neutral-100"
                          : "border-neutral-800 text-neutral-400 hover:border-neutral-700"
                      }`}
                    >
                      {mode}
                    </button>
                  ))}
                </div>
              </div>

              <p className="text-[11px] text-neutral-600">
                A theme is saved as soon as you pick it. Custom colours, fonts and
                sizes are the next round of work.
              </p>
            </>
          )}

          {tab === "api" && (
            <>
              <label className="block text-xs text-neutral-500">Base URL</label>
              <input
                className={inputCls}
                value={form.baseUrl}
                onChange={(e) => set("baseUrl", e.target.value)}
                spellCheck={false}
              />
              <label className="block text-xs text-neutral-500">
                API key {keyMask ? `(stored: ${keyMask})` : "(not set)"}
              </label>
              <div className="flex gap-2">
                <input
                  className={inputCls}
                  type="password"
                  placeholder="sk-…"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  spellCheck={false}
                />
                <button
                  onClick={saveKey}
                  disabled={busy || !apiKey.trim()}
                  className="shrink-0 rounded-lg bg-sky-600 px-3 text-sm font-medium text-white hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
                >
                  Save
                </button>
                <button
                  onClick={clearKey}
                  disabled={busy || !keyMask}
                  className="shrink-0 rounded-lg border border-neutral-800 px-3 text-sm text-neutral-400 hover:text-red-300 disabled:opacity-40"
                >
                  Clear
                </button>
              </div>
              <button
                onClick={testKey}
                disabled={busy}
                className="rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 hover:border-neutral-700"
              >
                Test connection
              </button>

              <div className="border-t border-neutral-800 pt-4">
                <label className="block text-xs text-neutral-500">
                  Documents folder (chat mode may read and write here)
                </label>
                <input
                  className={inputCls}
                  placeholder={documentsDir}
                  value={form.documentsDir}
                  onChange={(e) => set("documentsDir", e.target.value)}
                  spellCheck={false}
                />
                <p className="mt-1 text-[11px] text-neutral-600">
                  Leave blank to use the default: <span className="font-mono">{documentsDir}</span>
                </p>
              </div>
            </>
          )}

          {tab === "prompt" && (
            <>
              <label className="block text-xs text-neutral-500">Chat model</label>
              <select
                className={inputCls}
                value={form.chatModel}
                onChange={(e) => set("chatModel", e.target.value)}
              >
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
              <label className="block text-xs text-neutral-500">Coding model</label>
              <select
                className={inputCls}
                value={form.codingModel}
                onChange={(e) => set("codingModel", e.target.value)}
              >
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.label}
                  </option>
                ))}
              </select>
              <div className="flex items-center gap-3">
                <label className="flex items-center gap-2 text-xs text-neutral-400">
                  <input
                    type="checkbox"
                    checked={form.thinking}
                    onChange={(e) => set("thinking", e.target.checked)}
                  />
                  Thinking mode by default
                </label>
                <select
                  className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-none"
                  value={form.effort}
                  onChange={(e) => set("effort", e.target.value)}
                >
                  {EFFORTS.map((e) => (
                    <option key={e} value={e}>
                      {e}
                    </option>
                  ))}
                </select>
              </div>
              <label className="block text-xs text-neutral-500">Base system prompt</label>
              <textarea
                className={`${inputCls} h-32 resize-y`}
                value={form.systemPrompt}
                onChange={(e) => set("systemPrompt", e.target.value)}
              />
              <p className="text-[11px] text-neutral-600">
                MEMORY.md, enabled skills and any matching scoped instructions are appended to this
                automatically.
              </p>
            </>
          )}

          {tab === "browsing" && (
            <>
              <label className="flex items-center gap-2 text-xs text-neutral-300">
                <input
                  type="checkbox"
                  checked={form.browsingEnabled}
                  onChange={(e) => set("browsingEnabled", e.target.checked)}
                />
                Let the assistant use the internet
              </label>
              <p className="text-[11px] text-neutral-600">
                Three read-only tools, so nothing is prompted for approval.{" "}
                <span className="font-mono">fetch_url</span> reads a page as text.{" "}
                <span className="font-mono">browser_read</span> puts the page through a real
                browser first, so JavaScript-built pages work too.{" "}
                <span className="font-mono">web_search</span> searches.
              </p>

              <div className="border-t border-neutral-800 pt-4">
                <label className="block text-xs text-neutral-500">SearXNG URL</label>
                <input
                  className={inputCls}
                  placeholder="http://localhost:8080"
                  value={form.searxngUrl}
                  onChange={(e) => set("searxngUrl", e.target.value)}
                  spellCheck={false}
                />
                <p className="mt-1 text-[11px] text-neutral-600">
                  Leave blank to switch <span className="font-mono">web_search</span> off; the two
                  fetch tools keep working. The instance must list{" "}
                  <span className="font-mono">json</span> under <span className="font-mono">
                    formats
                  </span>{" "}
                  in its <span className="font-mono">settings.yml</span>, otherwise it only answers
                  with HTML.
                </p>
              </div>

              <div className="border-t border-neutral-800 pt-4">
                <label className="block text-xs text-neutral-500">
                  Browser for browser_read (optional)
                </label>
                <input
                  className={inputCls}
                  placeholder="auto-detect Edge, then Chrome"
                  value={form.browserPath}
                  onChange={(e) => set("browserPath", e.target.value)}
                  spellCheck={false}
                />
                <p className="mt-1 text-[11px] text-neutral-600">
                  Any Chromium-family browser is driven headlessly — nothing to download. Only set
                  this if auto-detection picks the wrong one.
                </p>
              </div>
            </>
          )}

          {tab === "instructions" && (
            <>
              <p className="text-xs text-neutral-500">
                Instructions apply only in their scope — so “when writing a document use this type”
                does not clutter every chat.
              </p>
              {form.instructions.map((instruction) => (
                <div
                  key={instruction.id}
                  className="space-y-2 rounded-lg border border-neutral-800 bg-neutral-900/40 p-3"
                >
                  <div className="flex items-center gap-2">
                    <input
                      className="flex-1 rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-none"
                      value={instruction.name}
                      onChange={(e) => setInstruction(instruction.id, { name: e.target.value })}
                    />
                    <select
                      className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-none"
                      value={instruction.scope}
                      onChange={(e) => setInstruction(instruction.id, { scope: e.target.value })}
                    >
                      {SCOPES.map((s) => (
                        <option key={s} value={s}>
                          {s}
                        </option>
                      ))}
                    </select>
                    <label className="flex items-center gap-1 text-[11px] text-neutral-400">
                      <input
                        type="checkbox"
                        checked={instruction.enabled}
                        onChange={(e) =>
                          setInstruction(instruction.id, { enabled: e.target.checked })
                        }
                      />
                      on
                    </label>
                    <button
                      onClick={() => removeInstruction(instruction.id)}
                      className="text-neutral-500 hover:text-red-400"
                      title="Remove"
                    >
                      ×
                    </button>
                  </div>
                  <textarea
                    className={`${inputCls} h-24 resize-y font-mono text-[11px]`}
                    value={instruction.body}
                    onChange={(e) => setInstruction(instruction.id, { body: e.target.value })}
                  />
                </div>
              ))}
              <button
                onClick={addInstruction}
                className="rounded-lg border border-neutral-800 px-3 py-1.5 text-xs text-neutral-300 hover:border-neutral-700"
              >
                + Add instruction
              </button>
            </>
          )}
        </div>

        <div className="flex items-center gap-3 border-t border-neutral-800 px-5 py-3">
          <span className="flex-1 truncate text-xs text-neutral-500">{status}</span>
          <button
            onClick={saveAll}
            disabled={busy}
            className="rounded-lg bg-neutral-100 px-3 py-1.5 text-sm font-medium text-neutral-900 hover:bg-white disabled:opacity-50"
          >
            Save
          </button>
          <button
            onClick={onClose}
            className="rounded-lg border border-neutral-800 px-3 py-1.5 text-sm text-neutral-300 hover:border-neutral-700"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
