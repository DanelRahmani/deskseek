import { useState } from "react";

import { api } from "../api";
import type { Dialect, Endpoint, ModelInfo, ModelPrice, Speech } from "../types";

const DIALECTS: { id: Dialect; label: string }[] = [
  { id: "openai-compatible", label: "OpenAI-compatible" },
  { id: "anthropic-compatible", label: "Anthropic-compatible" },
];

/**
 * Endpoints worth offering as one click, because the field is otherwise a URL a
 * user has to go and look up. Deliberately no model ids: what a vendor serves
 * this month is not something this app can know, and a list of invented ids
 * would be worse than an empty one.
 *
 * A `keyless` endpoint is one that needs no key — every model on localhost is,
 * which is the honest reason `keyRef` exists as a blank-able field.
 */
const PRESETS: {
  label: string;
  dialect: Dialect;
  baseUrl: string;
  keyless?: boolean;
  notes?: string;
}[] = [
  { label: "OpenRouter", dialect: "openai-compatible", baseUrl: "https://openrouter.ai/api/v1" },
  { label: "Groq", dialect: "openai-compatible", baseUrl: "https://api.groq.com/openai/v1" },
  { label: "Together", dialect: "openai-compatible", baseUrl: "https://api.together.xyz/v1" },
  { label: "xAI", dialect: "openai-compatible", baseUrl: "https://api.x.ai/v1" },
  {
    label: "Google",
    dialect: "openai-compatible",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
  },
  {
    label: "Ollama",
    dialect: "openai-compatible",
    baseUrl: "http://localhost:11434/v1",
    keyless: true,
    notes: "Runs on this machine. No key, and no published rate — its spend reads as unknown.",
  },
  {
    label: "LM Studio",
    dialect: "openai-compatible",
    baseUrl: "http://localhost:1234/v1",
    keyless: true,
    notes: "Runs on this machine. No key, and no published rate.",
  },
  {
    label: "vLLM",
    dialect: "openai-compatible",
    baseUrl: "http://localhost:8000/v1",
    keyless: true,
    notes: "Runs on this machine. No key, and no published rate.",
  },
  {
    label: "Anthropic",
    dialect: "anthropic-compatible",
    baseUrl: "https://api.anthropic.com",
  },
];

const inputCls =
  "w-full rounded-lg border border-neutral-800 bg-neutral-900 px-3 py-2 text-sm text-neutral-200 outline-hidden focus:border-neutral-700";
const smallCls =
  "w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-[11px] text-neutral-200 outline-hidden focus:border-neutral-700";

/** A slug for an id, so two endpoints called "Ollama" do not collide. */
function slug(label: string): string {
  const base = label
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  return base || "endpoint";
}

function uniqueId(taken: Endpoint[], label: string): string {
  const base = slug(label);
  if (!taken.some((endpoint) => endpoint.id === base)) return base;
  for (let n = 2; ; n += 1) {
    const candidate = `${base}-${n}`;
    if (!taken.some((endpoint) => endpoint.id === candidate)) return candidate;
  }
}

function blankModel(): ModelInfo {
  return {
    id: "",
    label: "",
    context: 128_000,
    maxOutput: 8_192,
    vision: false,
    effort: true,
    price: null,
  };
}

/**
 * The endpoint list: where each one is, which dialect it speaks, what it serves,
 * and which one a turn runs against.
 *
 * Every field here is saved with the rest of the settings by the dialog's own
 * Save button, except the key — which never travels through the window and so
 * has a Save of its own beside it.
 */
export function EndpointsSection({
  endpoints,
  defaultEndpoint,
  speech,
  onChange,
  onSpeechChange,
  save,
  onStatus,
  busy,
}: {
  endpoints: Endpoint[];
  defaultEndpoint: string;
  speech: Speech;
  onChange: (endpoints: Endpoint[], defaultEndpoint: string) => void;
  onSpeechChange: (speech: Speech) => void;
  /** Persists the whole settings form, which a test has to do before it runs. */
  save: () => Promise<void>;
  onStatus: (message: string) => void;
  busy: boolean;
}) {
  const [testing, setTesting] = useState<string | null>(null);

  const edit = (index: number, patch: Partial<Endpoint>) =>
    onChange(
      endpoints.map((endpoint, i) => (i === index ? { ...endpoint, ...patch } : endpoint)),
      defaultEndpoint,
    );

  const editModel = (index: number, at: number, patch: Partial<ModelInfo>) =>
    edit(index, {
      models: endpoints[index].models.map((model, i) => (i === at ? { ...model, ...patch } : model)),
    });

  const add = (preset?: (typeof PRESETS)[number]) => {
    const id = uniqueId(endpoints, preset?.label ?? "endpoint");
    const draft: Endpoint = {
      id,
      label: preset?.label ?? "New endpoint",
      dialect: preset?.dialect ?? "openai-compatible",
      baseUrl: preset?.baseUrl ?? "",
      keyRef: preset?.keyless ? "" : `key_${id}`,
      models: [],
      notes: preset?.notes ?? "",
    };
    // The first endpoint added to an empty list becomes the default, or there
    // would be nowhere to send anything.
    onChange([...endpoints, draft], endpoints.length ? defaultEndpoint : id);
  };

  const remove = (index: number) => {
    const gone = endpoints[index];
    const left = endpoints.filter((_, i) => i !== index);
    const nextDefault =
      defaultEndpoint === gone.id
        ? left[0]?.id ?? ""
        : defaultEndpoint;
    onChange(left, nextDefault);
  };

  const test = async (endpoint: Endpoint) => {
    setTesting(endpoint.id);
    onStatus(`Testing ${endpoint.label}…`);
    try {
      // Saved first: the key lives in the OS keychain against the endpoint's own
      // account, so an endpoint the app has never seen has no key to send.
      await save();
      await api.testEndpoint(endpoint.id);
      onStatus(`${endpoint.label} answered — the credentials work.`);
    } catch (e) {
      onStatus(`${endpoint.label} failed: ${e}`);
    } finally {
      setTesting(null);
    }
  };

  return (
    <div className="space-y-4">
      <p className="text-[11px] text-neutral-500">
        An endpoint is a base URL, a dialect, a key and the models it serves. Only two wire
        formats exist — OpenAI-compatible and Anthropic-compatible — so every vendor below is a
        row here rather than something the app had to be taught.
      </p>

      {endpoints.map((endpoint, index) => {
        const isDefault = endpoint.id === defaultEndpoint;
        return (
          <section
            key={endpoint.id}
            className={`rounded-xl border p-3 ${
              isDefault ? "border-sky-900 bg-sky-950/20" : "border-neutral-800"
            }`}
          >
            <div className="flex items-center gap-2">
              <label className="flex items-center gap-2 text-xs text-neutral-300">
                <input
                  type="radio"
                  name="default-endpoint"
                  checked={isDefault}
                  onChange={() => onChange(endpoints, endpoint.id)}
                />
                Use this one
              </label>
              <input
                className="min-w-0 flex-1 rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-sm text-neutral-200 outline-hidden focus:border-neutral-700"
                value={endpoint.label}
                onChange={(e) => edit(index, { label: e.target.value })}
                spellCheck={false}
                aria-label="Endpoint name"
              />
              <button
                onClick={() => remove(index)}
                disabled={endpoints.length < 2}
                className="shrink-0 rounded-lg border border-neutral-800 px-2 py-1 text-[11px] text-neutral-500 hover:text-red-300 disabled:opacity-30"
                title={endpoints.length < 2 ? "The last endpoint cannot be removed" : "Remove"}
              >
                Remove
              </button>
            </div>

            <div className="mt-2 grid grid-cols-2 gap-2">
              <div>
                <label className="block text-[11px] text-neutral-500">Dialect</label>
                <select
                  className={smallCls}
                  value={endpoint.dialect}
                  onChange={(e) => edit(index, { dialect: e.target.value as Dialect })}
                >
                  {DIALECTS.map((dialect) => (
                    <option key={dialect.id} value={dialect.id}>
                      {dialect.label}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-[11px] text-neutral-500">
                  Key reference {endpoint.keyRef.trim() ? "" : "(no key needed)"}
                </label>
                <input
                  className={smallCls}
                  value={endpoint.keyRef}
                  onChange={(e) => edit(index, { keyRef: e.target.value })}
                  spellCheck={false}
                  placeholder="key_…"
                />
              </div>
            </div>

            <label className="mt-2 block text-[11px] text-neutral-500">
              Base URL — without the dialect's own path (no <span className="font-mono">/chat/completions</span>)
            </label>
            <input
              className={inputCls}
              value={endpoint.baseUrl}
              onChange={(e) => edit(index, { baseUrl: e.target.value })}
              spellCheck={false}
              placeholder="https://api.example.com"
            />

            {endpoint.notes && (
              <p className="mt-1 text-[11px] text-neutral-600">{endpoint.notes}</p>
            )}

            <div className="mt-3 border-t border-neutral-800 pt-2">
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-neutral-400">
                  Models ({endpoint.models.length})
                </span>
                <button
                  onClick={() => edit(index, { models: [...endpoint.models, blankModel()] })}
                  className="rounded-md px-2 py-0.5 text-[11px] text-neutral-400 hover:bg-neutral-800 hover:text-neutral-200"
                >
                  Add model
                </button>
              </div>
              {endpoint.models.length === 0 && (
                <p className="mt-1 text-[11px] text-amber-500/80">
                  No models yet, so this endpoint cannot be picked. Add the ids it serves.
                </p>
              )}
              {endpoint.models.map((model, at) => (
                <div key={at} className="mt-2 rounded-lg border border-neutral-800/70 p-2">
                  <div className="flex gap-2">
                    <input
                      className={smallCls}
                      value={model.id}
                      onChange={(e) => editModel(index, at, { id: e.target.value })}
                      placeholder="model-id"
                      spellCheck={false}
                      aria-label="Model id"
                    />
                    <input
                      className={smallCls}
                      value={model.label}
                      onChange={(e) => editModel(index, at, { label: e.target.value })}
                      placeholder="Shown in the picker"
                      spellCheck={false}
                      aria-label="Model label"
                    />
                    <button
                      onClick={() =>
                        edit(index, { models: endpoint.models.filter((_, i) => i !== at) })
                      }
                      className="shrink-0 px-1 text-[11px] text-neutral-600 hover:text-red-300"
                      title="Remove this model"
                    >
                      ✕
                    </button>
                  </div>
                  <div className="mt-2 grid grid-cols-4 items-end gap-2">
                    <div>
                      <label className="block text-[10px] text-neutral-600">Context</label>
                      <input
                        className={smallCls}
                        type="number"
                        value={model.context}
                        onChange={(e) => editModel(index, at, { context: Number(e.target.value) })}
                      />
                    </div>
                    <div>
                      <label className="block text-[10px] text-neutral-600">Max output</label>
                      <input
                        className={smallCls}
                        type="number"
                        value={model.maxOutput}
                        onChange={(e) => editModel(index, at, { maxOutput: Number(e.target.value) })}
                      />
                    </div>
                    <label className="flex items-center gap-1 text-[11px] text-neutral-300">
                      <input
                        type="checkbox"
                        checked={model.vision}
                        onChange={(e) => editModel(index, at, { vision: e.target.checked })}
                      />
                      Can see images
                    </label>
                    <label
                      className="flex items-center gap-1 text-[11px] text-neutral-300"
                      title="Off means the chat shows no thinking controls and the request carries neither the level nor the switch"
                    >
                      <input
                        type="checkbox"
                        checked={model.effort}
                        onChange={(e) => editModel(index, at, { effort: e.target.checked })}
                      />
                      Takes an effort setting
                    </label>
                  </div>
                  <label
                    className="mt-2 flex items-center gap-1 text-[11px] text-neutral-300"
                    title="Off counts the tokens and reports the money as unknown rather than guessing"
                  >
                    <input
                      type="checkbox"
                      checked={model.price !== null}
                      onChange={(e) =>
                        editModel(index, at, {
                          price: e.target.checked
                            ? { hit: 0, miss: 0, out: 0, peakMultiplier: 1 }
                            : null,
                        })
                      }
                    />
                    Published rate
                  </label>
                  {model.price === null ? (
                    <p className="mt-1 text-[10px] text-neutral-600">
                      No rate published, so its tokens are counted and its cost reads as
                      <span className="text-neutral-400"> unknown</span> rather than zero.
                    </p>
                  ) : (
                    <div className="mt-2 grid grid-cols-4 gap-2">
                      {(["hit", "miss", "out"] as const).map((field) => (
                        <div key={field}>
                          <label className="block text-[10px] text-neutral-600">
                            {field === "hit" ? "Cache hit" : field === "miss" ? "Cache miss" : "Output"} / 1M
                          </label>
                          <input
                            className={smallCls}
                            type="number"
                            step="0.001"
                            value={model.price![field]}
                            onChange={(e) =>
                              editModel(index, at, {
                                price: { ...model.price!, [field]: Number(e.target.value) } as ModelPrice,
                              })
                            }
                          />
                        </div>
                      ))}
                      <div>
                        <label className="block text-[10px] text-neutral-600">Peak ×</label>
                        <input
                          className={smallCls}
                          type="number"
                          step="0.1"
                          value={model.price.peakMultiplier}
                          onChange={(e) =>
                            editModel(index, at, {
                              price: { ...model.price!, peakMultiplier: Number(e.target.value) },
                            })
                          }
                        />
                      </div>
                    </div>
                  )}
                </div>
              ))}
            </div>

            <button
              onClick={() => test(endpoint)}
              disabled={busy || testing !== null}
              className="mt-3 rounded-lg px-3 py-1 text-[12px] text-neutral-300 hover:bg-neutral-800 hover:text-neutral-100 disabled:opacity-40"
            >
              {testing === endpoint.id ? "Testing…" : "Save & test"}
            </button>
          </section>
        );
      })}

      <div className="border-t border-neutral-800 pt-3">
        <p className="mb-2 text-[11px] text-neutral-500">Add an endpoint</p>
        <div className="flex flex-wrap gap-1.5">
          {PRESETS.map((preset) => (
            <button
              key={preset.label}
              onClick={() => add(preset)}
              className="rounded-full border border-neutral-800 px-2.5 py-1 text-[11px] text-neutral-400 hover:border-neutral-700 hover:text-neutral-200"
            >
              {preset.label}
            </button>
          ))}
          <button
            onClick={() => add()}
            className="rounded-full border border-dashed border-neutral-700 px-2.5 py-1 text-[11px] text-neutral-500 hover:text-neutral-300"
          >
            Empty
          </button>
        </div>
      </div>

      {/* Voice is endpoint configuration, so it lives with the endpoints rather
          than in a section of its own. It is deliberately not a third dialect:
          the speech API is one shape whatever the endpoint speaks, so this
          names an endpoint, a model and a voice, and nothing more. */}
      <div className="space-y-2 border-t border-neutral-800 pt-3">
        <p className="text-[11px] font-medium tracking-wide text-neutral-400 uppercase">Voice</p>
        <p className="text-[11px] text-neutral-600">
          Audio overviews and dictation go to an endpoint that serves audio: OpenAI, Groq, or a
          local server. Leave the endpoint empty and both refuse rather than guess.
        </p>
        <div className="grid grid-cols-2 gap-2">
          <div>
            <label className="block text-[10px] text-neutral-600">Endpoint</label>
            <select
              className={smallCls}
              value={speech.endpoint}
              onChange={(e) => onSpeechChange({ ...speech, endpoint: e.target.value })}
            >
              <option value="">None</option>
              {endpoints.map((endpoint) => (
                <option key={endpoint.id} value={endpoint.id}>
                  {endpoint.label}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="block text-[10px] text-neutral-600">Speaks with (model)</label>
            <input
              className={smallCls}
              value={speech.ttsModel}
              placeholder="tts-1"
              onChange={(e) => onSpeechChange({ ...speech, ttsModel: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-[10px] text-neutral-600">Voice</label>
            <input
              className={smallCls}
              value={speech.voice}
              placeholder="alloy"
              onChange={(e) => onSpeechChange({ ...speech, voice: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-[10px] text-neutral-600">Published $ per minute</label>
            <input
              className={smallCls}
              type="number"
              step="0.001"
              value={speech.usdPerMinute}
              onChange={(e) =>
                onSpeechChange({ ...speech, usdPerMinute: Number(e.target.value) })
              }
            />
          </div>
        </div>
        {speech.usdPerMinute <= 0 && speech.endpoint !== "" && (
          <p className="text-[11px] text-amber-500/80">
            With no rate the clip is made and its cost is recorded as unknown, because a price
            nobody published is not zero.
          </p>
        )}
      </div>
    </div>
  );
}
