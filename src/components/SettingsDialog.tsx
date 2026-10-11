import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { THEMES, isHexColour } from "../lib/theme";
import { SHORTCUTS } from "../lib/shortcuts";
import { COMMANDS, MODES } from "./ChatView";
import { RUNG_LABEL, SANDBOX_MODES } from "./SetupPanel";
import type { EnvProbe, Instruction, ModelInfo, Settings } from "../types";

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

type Section =
  | "appearance"
  | "access"
  | "prompt"
  | "tools"
  | "browsing"
  | "instructions"
  | "commands";

/**
 * The rail. `keywords` is what the search box matches on, so looking for
 * "docker" finds Tools even though the word is not in its title.
 */
const SECTIONS: { id: Section; label: string; keywords: string }[] = [
  {
    id: "appearance",
    label: "Appearance",
    keywords: "theme colour color font size accent dark light motion density contrast",
  },
  { id: "access", label: "Access", keywords: "api key token base url endpoint documents folder" },
  {
    id: "prompt",
    label: "Models & prompt",
    keywords: "chat coding model thinking effort reasoning system prompt",
  },
  {
    id: "tools",
    label: "Tools",
    keywords: "docker wsl container sandbox shell command computer screen image",
  },
  {
    id: "browsing",
    label: "Browsing",
    keywords: "internet searxng search browser edge chrome fetch web",
  },
  { id: "instructions", label: "Instructions", keywords: "scoped rules context" },
  {
    id: "commands",
    label: "Commands",
    keywords: "slash keyboard shortcut permission mode guard help",
  },
];

/**
 * Accent swatches, chosen as solids dark enough to carry a white label and
 * mid-tone enough that the tints derived from them read on both a dark and a
 * light page. The free-text field beside them takes anything else.
 */
const ACCENTS = [
  "#0369a1",
  "#1d4ed8",
  "#6d28d9",
  "#be185d",
  "#b91c1c",
  "#c2410c",
  "#15803d",
  "#0f766e",
];

/**
 * Families worth offering, by where they come from. Not the machine's real
 * list — that is asked for at runtime below — but the families a machine is
 * likely to have, so the list is useful before anything is granted.
 */
const FONT_FAMILIES = [
  // Shipped with Windows
  "Arial", "Arial Black", "Bahnschrift", "Calibri", "Cambria", "Candara",
  "Comic Sans MS", "Consolas", "Constantia", "Corbel", "Courier New",
  "Franklin Gothic Medium", "Gabriola", "Georgia", "Impact", "Lucida Console",
  "Lucida Sans Unicode", "Microsoft Sans Serif", "Palatino Linotype", "Segoe Print",
  "Segoe Script", "Segoe UI", "Sitka Text", "Sylfaen", "Tahoma", "Times New Roman",
  "Trebuchet MS", "Verdana",
  // Shipped with macOS
  "American Typewriter", "Avenir", "Avenir Next", "Baskerville", "Charter", "Didot",
  "Futura", "Geneva", "Gill Sans", "Helvetica", "Helvetica Neue", "Hiragino Sans",
  "Iowan Old Style", "Lucida Grande", "Menlo", "Monaco", "Optima", "Palatino",
  "SF Mono", "San Francisco", "Skia",
  // Shipped with the common Linux desktops
  "Cantarell", "DejaVu Sans", "DejaVu Sans Mono", "Liberation Mono", "Liberation Sans",
  "Liberation Serif", "Noto Sans", "Noto Sans Mono", "Ubuntu", "Ubuntu Mono",
  // Coding fonts, installed on purpose
  "Cascadia Code", "Cascadia Mono", "Fira Code", "Hack", "IBM Plex Mono", "Iosevka",
  "JetBrains Mono", "Roboto Mono", "Source Code Pro", "Space Mono", "Victor Mono",
  // Free faces common enough to be worth listing
  "Fira Sans", "IBM Plex Sans", "Inter", "Lato", "Merriweather", "Montserrat",
  "Open Sans", "Poppins", "Roboto", "Source Sans Pro",
].sort((a, b) => a.localeCompare(b));

/** What `queryLocalFonts` hands back, narrowed to the one field used. */
type LocalFont = { family: string };

/**
 * Every family this machine has, asked for once and cached in the module so the
 * second font field does not raise a second permission prompt.
 *
 * ponytail: the promise is the cache, so a refusal stays refused for the life
 * of the session. If a webview ever grants it later, drop the cache and the
 * retry comes free.
 */
let localFonts: Promise<string[]> | null = null;
function loadLocalFonts(): Promise<string[]> {
  localFonts ??= (async () => {
    const read = (navigator as Navigator & { queryLocalFonts?: () => Promise<LocalFont[]> })
      .queryLocalFonts;
    if (!read) throw new Error("this webview does not expose the local font list");
    // `call` keeps the receiver: the method throws when detached from navigator.
    const fonts = await read.call(navigator);
    return [...new Set(fonts.map((font) => font.family))];
  })();
  return localFonts;
}

const FONT_SIZES = [12, 13, 14, 15, 16, 18];

/**
 * A family name as a CSS value, with the live token behind it so a family that
 * is not installed falls back to what is really in use instead of to the
 * browser's default serif.
 *
 * Quotes are stripped first: a stray one makes the whole declaration invalid,
 * and an invalid `font-family` fails silently — the field would simply stop
 * previewing anything with no sign of why.
 */
function previewFamily(family: string, fallback: string): string | undefined {
  const name = family.trim().replace(/"/g, "");
  if (!name) return undefined;
  return `"${name}", ${fallback}`;
}

/**
 * A font field with a list under it, every entry set in the family it names.
 *
 * Deliberately not a `<datalist>`: that filters by what is already in the box,
 * so with the current family sitting there the only entry left is the one you
 * have, and changing font means clearing the field first. Here the list opens
 * whole — the current family included — and narrows only once you type.
 */
function FontPicker({
  value,
  fallback,
  placeholder,
  onPick,
}: {
  value: string;
  fallback: string;
  placeholder: string;
  onPick: (family: string) => void;
}) {
  // null means "showing what is saved"; a string means the box is being edited,
  // which is also what decides whether the list filters.
  const [draft, setDraft] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [installed, setInstalled] = useState<string[]>([]);
  const needle = draft?.trim().toLowerCase() ?? "";

  const families = useMemo(() => {
    const all = installed.length
      ? [...new Set([...FONT_FAMILIES, ...installed])].sort((a, b) => a.localeCompare(b))
      : FONT_FAMILIES;
    return needle ? all.filter((family) => family.toLowerCase().includes(needle)) : all;
  }, [installed, needle]);

  const commit = (family: string) => {
    setDraft(null);
    setOpen(false);
    const next = family.trim();
    if (next !== value) onPick(next);
  };

  return (
    <div>
      <input
        value={draft ?? value}
        onChange={(e) => {
          setDraft(e.target.value);
          setOpen(true);
        }}
        onFocus={() => {
          setOpen(true);
          // Asking needs a user gesture, and a focus is one. A refusal is not
          // worth reporting: the curated list carries on by itself.
          loadLocalFonts()
            .then(setInstalled)
            .catch(() => {});
        }}
        onBlur={() => (draft === null ? setOpen(false) : commit(draft))}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            setDraft(null);
            setOpen(false);
          } else if (e.key === "Enter") {
            commit(draft ?? value);
          } else if (e.key === "ArrowDown" && !open) {
            setOpen(true);
          }
        }}
        placeholder={placeholder}
        spellCheck={false}
        // The field previews its own family: a font is impossible to judge in a
        // font that is not the one being named.
        style={{ fontFamily: previewFamily(draft ?? value, fallback) }}
        className={inputCls}
      />
      {open && (
        <div className="mt-1 max-h-56 overflow-y-auto rounded-lg border border-neutral-800 bg-neutral-950 py-1">
          {families.map((family) => (
            <button
              key={family}
              type="button"
              // Holding the field's focus, so picking a row is not a blur that
              // commits the half-typed name first.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => commit(family)}
              style={{ fontFamily: previewFamily(family, fallback) }}
              className={`block w-full truncate px-3 py-1 text-left text-sm hover:bg-neutral-800 ${
                family === value ? "text-sky-300" : "text-neutral-300"
              }`}
            >
              {family}
            </button>
          ))}
          {families.length === 0 && (
            <p className="px-3 py-1 text-[11px] text-neutral-600">
              Nothing here matches. What you type is kept either way.
            </p>
          )}
        </div>
      )}
    </div>
  );
}

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
  const [tab, setTab] = useState<Section>("appearance");
  const [query, setQuery] = useState("");
  const [probe, setProbe] = useState<EnvProbe | null>(null);
  // Text fields commit on blur rather than on every keystroke: saving per letter
  // would rewrite the settings file dozens of times to say nothing new.
  const [accentDraft, setAccentDraft] = useState(settings.accent);

  // The Tools and Browsing sections report what this machine actually has, so
  // the probe runs once when the dialog opens; Re-check asks again.
  const refreshProbe = async () => {
    try {
      setProbe(await api.probeEnvironment());
    } catch (e) {
      setStatus(String(e));
    }
  };

  useEffect(() => {
    refreshProbe();
    // Once, on open. A probe takes a second or two, so this must not be a loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Search filters the rail, and a query that hides the open section moves to
  // the first one still showing — otherwise the rail and the body disagree.
  const needle = query.trim().toLowerCase();
  const visible = needle
    ? SECTIONS.filter((s) => `${s.label} ${s.keywords}`.toLowerCase().includes(needle))
    : SECTIONS;
  useEffect(() => {
    if (visible.length > 0 && !visible.some((s) => s.id === tab)) setTab(visible[0].id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [needle]);

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
   * Save one setting and ask the machine again.
   *
   * For the controls the Tools section then reports on: the resolved rung
   * depends on the saved value, and clicking a sandbox mode while the panel went
   * on naming the old rung would be worse than not showing it at all.
   */
  const saveTool = async (patch: Partial<Settings>) => {
    const next = { ...form, ...patch };
    setForm(next);
    try {
      onSaved(await api.saveSettings(next), keyMask);
      await refreshProbe();
    } catch (e) {
      setStatus(String(e));
    }
  };

  /**
   * Picking a theme saves it straight away rather than waiting for the Save
   * button: a theme you cannot see until you press Save is not a theme picker,
   * and the app repaints from the settings this hands back to it.
   */
  const pickAppearance = async (
    patch: Partial<
      Pick<
        Settings,
        | "theme"
        | "themeMode"
        | "accent"
        | "fontUi"
        | "fontCode"
        | "fontSize"
        | "reduceMotion"
      >
    >,
  ) => {
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
        <div className="flex shrink-0 items-center gap-3 border-b border-neutral-800 px-5 py-3">
          <h2 className="text-sm font-semibold">Settings</h2>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search settings…"
            spellCheck={false}
            className="ml-auto w-52 rounded-lg border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200 outline-none focus:border-neutral-700"
          />
          <button onClick={onClose} className="text-neutral-500 hover:text-neutral-200">
            ×
          </button>
        </div>

        <div className="flex min-h-0 flex-1">
          <nav className="flex w-48 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-neutral-800 p-2">
            {visible.map((section) => (
              <button
                key={section.id}
                onClick={() => setTab(section.id)}
                className={`rounded-md px-3 py-2 text-left text-xs transition ${
                  tab === section.id
                    ? "bg-neutral-800 font-medium text-neutral-100"
                    : "text-neutral-500 hover:bg-neutral-900 hover:text-neutral-300"
                }`}
              >
                {section.label}
              </button>
            ))}
            {visible.length === 0 && (
              <p className="px-3 py-2 text-[11px] text-neutral-600">Nothing matches.</p>
            )}
          </nav>

          <div className="min-w-0 flex-1 space-y-5 overflow-y-auto px-5 py-4">
          {tab === "appearance" && (
            <>
              <section>
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
                      {/* Both variants, because a theme is chosen for how it looks
                          in both modes and not only the one you happen to be in. */}
                      <span className="mb-1.5 flex gap-1" title="Dark">
                        {[theme.dark.ramp.n950, theme.dark.ramp.n800, theme.dark.accent].map(
                          (colour) => (
                            <span
                              key={colour}
                              className="h-4 w-4 rounded-sm"
                              style={{ background: colour }}
                            />
                          ),
                        )}
                      </span>
                      <span className="mb-2 flex gap-1" title="Light">
                        {[theme.light.ramp.n950, theme.light.ramp.n800, theme.light.accent].map(
                          (colour) => (
                            <span
                              key={colour}
                              className="h-4 w-4 rounded-sm ring-1 ring-neutral-800"
                              style={{ background: colour }}
                            />
                          ),
                        )}
                      </span>
                      {theme.name}
                    </button>
                  ))}
                </div>
              </section>

              <section>
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
              </section>

              <section>
                <label className="block text-xs text-neutral-500">Accent</label>
                <div className="mt-2 flex flex-wrap items-center gap-1.5">
                  <button
                    onClick={() => {
                      setAccentDraft("");
                      pickAppearance({ accent: "" });
                    }}
                    title="Use the theme's own accent"
                    className={`h-6 rounded-md border px-2 text-[11px] transition ${
                      form.accent
                        ? "border-neutral-800 text-neutral-400 hover:border-neutral-700"
                        : "border-sky-500 text-neutral-100"
                    }`}
                  >
                    Theme's own
                  </button>
                  {ACCENTS.map((colour) => (
                    <button
                      key={colour}
                      onClick={() => {
                        setAccentDraft(colour);
                        pickAppearance({ accent: colour });
                      }}
                      title={colour}
                      aria-label={`Accent ${colour}`}
                      style={{ background: colour }}
                      className={`h-6 w-6 rounded-md ring-2 transition ${
                        form.accent.toLowerCase() === colour ? "ring-sky-400" : "ring-transparent"
                      }`}
                    />
                  ))}
                  <input
                    value={accentDraft}
                    onChange={(e) => setAccentDraft(e.target.value)}
                    onBlur={() =>
                      accentDraft !== form.accent && pickAppearance({ accent: accentDraft })
                    }
                    placeholder="#0369a1"
                    spellCheck={false}
                    className="w-24 rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 font-mono text-[11px] text-neutral-200 outline-none focus:border-neutral-700"
                  />
                </div>
                {accentDraft.trim() !== "" && !isHexColour(accentDraft) && (
                  <p className="mt-1 text-[11px] text-amber-300">
                    Not a colour — six hex digits, like #0369a1. Until then the theme's own
                    accent is used.
                  </p>
                )}
              </section>

              <section>
                <label className="block text-xs text-neutral-500">Fonts</label>
                <div className="mt-2 space-y-2">
                  <FontPicker
                    value={form.fontUi}
                    fallback="var(--font-ui)"
                    placeholder="Interface font — blank uses the built-in stack"
                    onPick={(fontUi) => pickAppearance({ fontUi })}
                  />
                  <FontPicker
                    value={form.fontCode}
                    fallback="var(--font-code)"
                    placeholder="Code font — blank uses the built-in stack"
                    onPick={(fontCode) => pickAppearance({ fontCode })}
                  />
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-neutral-500">Text size</span>
                    <select
                      value={String(form.fontSize)}
                      onChange={(e) => pickAppearance({ fontSize: Number(e.target.value) })}
                      className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-300 outline-none"
                    >
                      <option value="0">Default</option>
                      {FONT_SIZES.map((size) => (
                        <option key={size} value={size}>
                          {size}px
                        </option>
                      ))}
                    </select>
                  </div>

                  {/* Each family named in itself: the name is the one string
                      that says which typeface you are reading, and the code
                      line carries the glyph pairs a code font has to keep
                      apart. The wrapper sets the size so the code line can be
                      the same 0.9em that the app's own code blocks use. */}
                  <div
                    className="rounded-lg border border-neutral-800 bg-neutral-900/40 px-3 py-2"
                    style={{ fontSize: form.fontSize > 0 ? `${form.fontSize}px` : undefined }}
                  >
                    <p
                      className="truncate text-neutral-200"
                      style={{ fontFamily: previewFamily(form.fontUi, "var(--font-ui)") }}
                    >
                      {`${form.fontUi.trim() || "Interface default"} — the quick brown fox jumps over the lazy dog`}
                    </p>
                    <p
                      className="mt-1 truncate text-[0.9em] text-neutral-400"
                      style={{ fontFamily: previewFamily(form.fontCode, "var(--font-code)") }}
                    >
                      {`${form.fontCode.trim() || "Code default"} — const done = items.filter((item) => item.done).length; // 0O 1lI`}
                    </p>
                  </div>
                </div>
                <p className="mt-1 text-[11px] text-neutral-600">
                  A family is used exactly as typed, so one that is not installed falls back to the
                  built-in stack. Anything on this machine can be typed in, whether or not the list
                  knows it. Nothing is downloaded.
                </p>
              </section>

              <section>
                <label className="flex items-center gap-2 text-xs text-neutral-300">
                  <input
                    type="checkbox"
                    checked={form.reduceMotion}
                    onChange={(e) => pickAppearance({ reduceMotion: e.target.checked })}
                  />
                  Reduce motion
                </label>
                <p className="mt-1 text-[11px] text-neutral-600">
                  Stops transitions and the pulsing indicators. Your operating system's own
                  reduced-motion preference is honoured either way.
                </p>
              </section>

              <p className="text-[11px] text-neutral-600">
                Appearance choices are saved the moment you make them.
              </p>
            </>
          )}

          {tab === "access" && (
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
                  className="shrink-0 rounded-lg bg-sky-600 px-3 text-sm font-medium text-accent-fg hover:bg-sky-500 disabled:bg-neutral-800 disabled:text-neutral-600"
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
                <div className="mt-2">
                  <button
                    onClick={async () => {
                      try {
                        await api.revealPath(documentsDir);
                      } catch (e) {
                        setStatus(String(e));
                      }
                    }}
                    className={quietBtn}
                  >
                    Show that folder
                  </button>
                </div>
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
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <Status
                    state={probe?.browserName ? "ok" : "todo"}
                    label={probe?.browserName ? `${probe.browserName} found` : "none found"}
                  />
                  {probe?.browserPath && (
                    <span
                      className="truncate font-mono text-[11px] text-neutral-600"
                      title={probe.browserPath}
                    >
                      {probe.browserPath}
                    </span>
                  )}
                  <button onClick={refreshProbe} disabled={busy} className={quietBtn}>
                    Re-check
                  </button>
                </div>
                <input
                  className={`${inputCls} mt-2`}
                  placeholder="auto-detect Edge, then Chrome"
                  value={form.browserPath}
                  onChange={(e) => set("browserPath", e.target.value)}
                  spellCheck={false}
                />
                <p className="mt-1 text-[11px] text-neutral-600">
                  Any Chromium-family browser is driven headlessly — nothing to download. Only set
                  this if auto-detection picks the wrong one; press Re-check after saving to see
                  the change take.
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

          {tab === "tools" && (
            <>
              <section>
                <label className="block text-xs text-neutral-500">Where shell commands run</label>
                <div className="mt-2 flex flex-wrap items-center gap-0.5 rounded-md border border-neutral-800 p-0.5">
                  {SANDBOX_MODES.map((mode) => (
                    <button
                      key={mode.id}
                      onClick={() => saveTool({ sandbox: mode.id })}
                      title={mode.hint}
                      className={`rounded px-2 py-1 text-[11px] transition ${
                        form.sandbox === mode.id
                          ? "bg-neutral-800 text-neutral-100"
                          : "text-neutral-500 hover:text-neutral-300"
                      }`}
                    >
                      {mode.label}
                    </button>
                  ))}
                </div>
                <p className="mt-2 text-xs text-neutral-500">
                  Commands currently run in{" "}
                  <strong className="text-neutral-300">
                    {RUNG_LABEL[probe?.sandboxRung ?? "host"]}
                  </strong>
                  . A container and a WSL distribution cannot see the rest of your computer; a
                  container also has no network and only the tools its image was built with.
                </p>
                {probe?.sandboxNote && (
                  <p className="mt-1 text-[11px] text-amber-300">{probe.sandboxNote}</p>
                )}
                {(form.sandbox === "docker" || probe?.sandboxRung === "docker") && (
                  <div className="mt-2">
                    <label className="block text-[11px] text-neutral-600">
                      Image — it must carry the workspace's toolchain
                    </label>
                    <input
                      value={form.sandboxImage}
                      onChange={(e) => set("sandboxImage", e.target.value)}
                      onBlur={() => saveTool({ sandboxImage: form.sandboxImage })}
                      placeholder="alpine:latest"
                      spellCheck={false}
                      className="mt-1 w-full rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 font-mono text-[11px] text-neutral-200 outline-none focus:border-neutral-700"
                    />
                  </div>
                )}
              </section>

              <section>
                <label className="block text-xs text-neutral-500">Docker</label>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <Status
                    state={probe?.dockerRunning ? "ok" : probe?.dockerCli ? "warn" : "todo"}
                    label={
                      probe?.dockerRunning
                        ? "engine running"
                        : probe?.dockerCli
                          ? "installed, engine stopped"
                          : "not found"
                    }
                  />
                  {probe?.dockerVersion && (
                    <span className="truncate font-mono text-[11px] text-neutral-600">
                      {probe.dockerVersion}
                    </span>
                  )}
                </div>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  {probe?.dockerDesktop && !probe?.dockerRunning && (
                    <button
                      onClick={async () => {
                        setBusy(true);
                        try {
                          await api.startDocker();
                          setStatus(
                            "Docker Desktop is starting. The engine answers a minute or so later — then press Re-check.",
                          );
                        } catch (e) {
                          setStatus(String(e));
                        } finally {
                          setBusy(false);
                        }
                      }}
                      disabled={busy}
                      className="rounded-md bg-sky-600 px-2 py-1 text-xs font-medium text-accent-fg hover:bg-sky-500 disabled:opacity-50"
                    >
                      Start Docker Desktop
                    </button>
                  )}
                  <button onClick={refreshProbe} disabled={busy} className={quietBtn}>
                    Re-check
                  </button>
                  <button
                    onClick={() =>
                      openUrl("https://docs.docker.com/desktop/install/windows-install/")
                    }
                    className={quietBtn}
                  >
                    Install guide
                  </button>
                </div>
                <p className="mt-2 text-[11px] text-neutral-600">
                  DeskSeek never installs Docker. A stopped engine is the usual reason a container
                  command quietly falls back to running on your machine.
                </p>
              </section>

              <section>
                <label className="block text-xs text-neutral-500">WSL2</label>
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <Status
                    state={probe?.wsl ? "ok" : "todo"}
                    label={probe?.wsl ? "found" : "not found"}
                  />
                  {probe?.wslDistro && (
                    <span className="font-mono text-[11px] text-neutral-600">
                      {probe.wslDistro}
                    </span>
                  )}
                  <button
                    onClick={() => openUrl("https://learn.microsoft.com/windows/wsl/install")}
                    className={quietBtn}
                  >
                    Install guide
                  </button>
                </div>
                <p className="mt-2 text-[11px] text-neutral-600">
                  A second way to isolate commands, for machines without Docker.
                </p>
              </section>

              <section>
                <label className="flex items-center gap-2 text-xs text-neutral-300">
                  <input
                    type="checkbox"
                    checked={form.computerUse}
                    onChange={(e) => saveTool({ computerUse: e.target.checked })}
                  />
                  Let the assistant read and operate the desktop
                </label>
                <p className="mt-1 text-[11px] text-neutral-600">
                  It asks before every press and every keystroke, and Ctrl+Alt+Esc stops it
                  immediately. Off by default, because whatever is on screen may be private.
                  Windows only.
                </p>
              </section>
            </>
          )}

          {tab === "commands" && (
            <>
              <section>
                <label className="block text-xs text-neutral-500">Slash commands</label>
                <p className="mt-1 text-[11px] text-neutral-600">
                  Typed in the message box. The menu appears as soon as you type a slash.
                </p>
                <ul className="mt-2 space-y-1">
                  {COMMANDS.map((command) => (
                    <li key={command.name} className="flex gap-3 text-xs">
                      <span className="w-24 shrink-0 font-mono text-neutral-300">
                        {command.name}
                      </span>
                      <span className="text-neutral-500">{command.description}</span>
                    </li>
                  ))}
                </ul>
              </section>

              <section>
                <label className="block text-xs text-neutral-500">Keyboard</label>
                <ul className="mt-2 space-y-1">
                  {SHORTCUTS.map((shortcut) => (
                    <li key={shortcut.action} className="flex gap-3 text-xs">
                      <span className="w-24 shrink-0 font-mono text-neutral-300">
                        {shortcut.keys}
                      </span>
                      <span className="text-neutral-500">{shortcut.label}</span>
                    </li>
                  ))}
                </ul>
                <p className="mt-1 text-[11px] text-neutral-600">Ctrl works wherever Cmd does.</p>
              </section>

              <section>
                <label className="block text-xs text-neutral-500">How much it asks</label>
                <ul className="mt-2 space-y-1">
                  {MODES.map((mode) => (
                    <li key={mode.id} className="flex gap-3 text-xs">
                      <span className="w-24 shrink-0 text-neutral-300">{mode.label}</span>
                      <span className="text-neutral-500">{mode.hint}</span>
                    </li>
                  ))}
                </ul>
                <p className="mt-2 text-[11px] text-neutral-600">
                  Chosen per chat in the header. The guard is a safety net, not a security
                  boundary — only a container or a WSL distribution separates a command from the
                  rest of the machine.
                </p>
              </section>
            </>
          )}
          </div>
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

const quietBtn =
  "rounded-md border border-neutral-800 px-2 py-1 text-xs text-neutral-300 transition hover:border-neutral-700 disabled:opacity-50";

/**
 * A state chip for the Tools section, so what a machine has reads as a fact
 * rather than as a field somebody has to interpret.
 */
function Status({ state, label }: { state: "ok" | "warn" | "todo"; label: string }) {
  const tone =
    state === "ok"
      ? "bg-emerald-500/10 text-emerald-400"
      : state === "warn"
        ? "bg-amber-500/10 text-amber-400"
        : "bg-neutral-800 text-neutral-400";
  return (
    <span className={`rounded px-1.5 py-0.5 text-[10px] ${tone}`}>
      {state === "ok" ? "✓ " : ""}
      {label}
    </span>
  );
}
