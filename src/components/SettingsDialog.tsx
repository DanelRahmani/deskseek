import { useState } from "react";
import { api } from "../api";
import type { ModelInfo, Settings } from "../types";

interface Props {
  settings: Settings;
  keyMask: string | null;
  models: ModelInfo[];
  onClose: () => void;
  onSaved: (settings: Settings, keyMask: string | null) => void;
}

const EFFORTS = ["low", "high", "max"];
const inputCls =
  "w-full rounded-lg border border-neutral-800 bg-neutral-900 px-3 py-2 text-sm text-neutral-200 outline-none focus:border-neutral-700";

export function SettingsDialog({ settings, keyMask, models, onClose, onSaved }: Props) {
  const [form, setForm] = useState<Settings>(settings);
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  const saveKey = async () => {
    if (!apiKey.trim()) return;
    setBusy(true);
    setStatus(null);
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

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/60 p-4">
      <div className="flex max-h-[85vh] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-neutral-800 bg-neutral-950">
        <div className="flex items-center justify-between border-b border-neutral-800 px-5 py-3">
          <h2 className="text-sm font-semibold">Settings</h2>
          <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200">
            ×
          </button>
        </div>

        <div className="flex-1 space-y-5 overflow-y-auto px-5 py-4">
          <section className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
              DeepSeek API
            </h3>
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
          </section>

          <section className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
              Defaults
            </h3>
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
            <div className="flex items-center gap-3 pt-1">
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
          </section>

          <section className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
              System prompt
            </h3>
            <textarea
              className={`${inputCls} h-28 resize-y`}
              value={form.systemPrompt}
              onChange={(e) => set("systemPrompt", e.target.value)}
            />
            <p className="text-[11px] text-neutral-600">
              The memory file is appended to this automatically, if it exists.
            </p>
          </section>
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
