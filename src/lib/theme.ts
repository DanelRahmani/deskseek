/**
 * Themes, as data.
 *
 * A theme is two variants — dark and light — and a variant is a ten-step grey
 * ramp plus four status hues. Everything else (a status colour's tint, border,
 * label and hover) is derived, so a new theme costs fourteen values rather than
 * thirty. Solarized is deliberately not here: its palette is built to be low
 * contrast and cannot clear the readability floors below without ceasing to be
 * Solarized.
 *
 * resolveTheme() returns the custom properties App.tsx writes onto <html>.
 * index.css holds only the Slate-dark defaults — the values that paint before
 * settings have loaded — and maps Tailwind's own colour names onto these
 * tokens, which is what restyles every existing component without editing it.
 * theme.check.ts fails the build if the two drift.
 */

/** A ten-step grey ramp. `n950` is the darkest (the page), `n100` the brightest (text). */
export interface Ramp {
  n950: string;
  n900: string;
  n800: string;
  n700: string;
  n600: string;
  n500: string;
  n400: string;
  n300: string;
  n200: string;
  n100: string;
}

export interface Variant {
  ramp: Ramp;
  /** The saturated hue behind accent buttons and active tabs. */
  accent: string;
  danger: string;
  warn: string;
  ok: string;
}

export interface Theme {
  id: string;
  name: string;
  dark: Variant;
  light: Variant;
}

/** `system` follows the OS; the other two override it. */
export type ThemeMode = "system" | "dark" | "light";

export const THEMES: readonly Theme[] = [
  {
    id: "slate",
    name: "Slate",
    dark: {
      ramp: {
        n950: "#0a0a0a",
        n900: "#171717",
        n800: "#262626",
        n700: "#404040",
        // n-600 is the faintness floor, not a taste call: today's #525252 reads
        // at 2.53:1 on the page, which is below the 3:1 this tier is held to.
        n600: "#6a6a6a",
        n500: "#737373",
        n400: "#a3a3a3",
        n300: "#d4d4d4",
        n200: "#e5e5e5",
        n100: "#f5f5f5",
      },
      accent: "#0284c7",
      danger: "#dc2626",
      warn: "#d97706",
      ok: "#059669",
    },
    light: {
      ramp: {
        n950: "#ffffff",
        n900: "#f6f7f9",
        n800: "#e4e6ea",
        n700: "#c9ccd3",
        n600: "#6b7280",
        n500: "#858c98",
        n400: "#5c626d",
        n300: "#3f444d",
        n200: "#24272d",
        n100: "#0f1115",
      },
      accent: "#0369a1",
      danger: "#b91c1c",
      warn: "#b45309",
      ok: "#15803d",
    },
  },
  {
    id: "nord",
    name: "Nord",
    dark: {
      ramp: {
        n950: "#2e3440",
        n900: "#3b4252",
        n800: "#434c5e",
        n700: "#4c566a",
        n600: "#8b98b0",
        n500: "#9aa6bd",
        n400: "#b3bfd2",
        n300: "#c8d1e0",
        n200: "#d8dee9",
        n100: "#eceff4",
      },
      accent: "#5e81ac",
      danger: "#bf616a",
      warn: "#b8803a",
      ok: "#5f8b52",
    },
    light: {
      ramp: {
        n950: "#ffffff",
        n900: "#f1f4f9",
        n800: "#dfe5ee",
        n700: "#c2cbd9",
        n600: "#5f6b80",
        n500: "#78849b",
        n400: "#4b5768",
        n300: "#3b4355",
        n200: "#2e3440",
        n100: "#1f242e",
      },
      accent: "#4c6f96",
      danger: "#a9444d",
      warn: "#8f6420",
      ok: "#4f7a45",
    },
  },
  {
    id: "gruvbox",
    name: "Gruvbox",
    dark: {
      ramp: {
        n950: "#282828",
        n900: "#3c3836",
        n800: "#504945",
        n700: "#665c54",
        n600: "#95866f",
        n500: "#a89984",
        n400: "#bdae93",
        n300: "#d5c4a1",
        n200: "#ebdbb2",
        n100: "#fbf1c7",
      },
      accent: "#458588",
      danger: "#cc241d",
      warn: "#c07d16",
      ok: "#7d7c15",
    },
    light: {
      ramp: {
        n950: "#fbf1c7",
        n900: "#f2e5bc",
        n800: "#ded0a4",
        n700: "#c3b394",
        n600: "#7a6f57",
        n500: "#8b7e66",
        n400: "#675c43",
        n300: "#4a4132",
        n200: "#3c3836",
        n100: "#282828",
      },
      accent: "#076678",
      danger: "#9d0006",
      warn: "#9a6410",
      ok: "#6c6a0c",
    },
  },
];

export const DEFAULT_THEME_ID = "slate";

export function themeById(id: string): Theme {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

/** Anything that is not explicitly dark or light follows the OS. */
function normaliseMode(mode: string): ThemeMode {
  return mode === "dark" || mode === "light" ? mode : "system";
}

/** The mode a choice really means, once `system` has been resolved. */
export function resolveMode(mode: string, systemDark: boolean): "dark" | "light" {
  const chosen = normaliseMode(mode);
  if (chosen === "system") return systemDark ? "dark" : "light";
  return chosen;
}

/**
 * The custom properties for one theme, all of them concrete hex.
 *
 * They are computed here rather than mixed in CSS so the contrast floors in
 * theme.check.ts can be measured on the real values — a `color-mix()` in the
 * stylesheet would be unmeasurable without a browser.
 */
export function resolveTheme(
  themeId: string,
  mode: string,
  systemDark: boolean,
): Record<string, string> {
  const theme = themeById(themeId);
  const resolved = resolveMode(mode, systemDark);
  const variant = resolved === "dark" ? theme.dark : theme.light;
  const { ramp } = variant;
  return {
    "--n-950": ramp.n950,
    "--n-900": ramp.n900,
    "--n-800": ramp.n800,
    "--n-700": ramp.n700,
    "--n-600": ramp.n600,
    "--n-500": ramp.n500,
    "--n-400": ramp.n400,
    "--n-300": ramp.n300,
    "--n-200": ramp.n200,
    "--n-100": ramp.n100,
    ...status("accent", variant.accent, ramp, resolved),
    ...status("danger", variant.danger, ramp, resolved),
    ...status("warn", variant.warn, ramp, resolved),
    ...status("ok", variant.ok, ramp, resolved),
  };
}

/**
 * How far a status label is lifted toward the ramp's text end.
 *
 * The two modes need different amounts, measured rather than guessed: a dark
 * theme's panel is only a step or two from its page, so its labels need a long
 * lift to reach 4.5:1, while a light theme clears the floor with a short one and
 * goes muddy — a dark grey-blue instead of the theme's hue — if pushed as far.
 */
const TEXT_LIFT = { dark: 0.55, light: 0.4 } as const;

/**
 * One status hue, as the five values the app's Tailwind classes collapse onto.
 *
 * The same mix rules serve both modes because they mix toward the ramp's ends:
 * on a dark theme `n100` is near-white, so `-text` lightens and `-hover` lifts;
 * on a light theme `n100` is near-black, so the very same rules darken. That is
 * one rule instead of two, and the contrast check proves it lands.
 */
function status(
  prefix: string,
  solid: string,
  ramp: Ramp,
  mode: "dark" | "light",
): Record<string, string> {
  return {
    // A whisper of the hue laid on the page, and a dimmer one for its edge:
    // both are read through the alpha the components already apply, so mixing
    // toward the page keeps `bg-amber-950/40` as subtle as it is today.
    [`--${prefix}-bg`]: mix(ramp.n950, solid, 0.22),
    [`--${prefix}-border`]: mix(solid, ramp.n950, 0.62),
    [`--${prefix}-text`]: mix(solid, ramp.n100, TEXT_LIFT[mode]),
    [`--${prefix}-solid`]: solid,
    [`--${prefix}-hover`]: mix(solid, ramp.n100, 0.18),
  };
}

/** `#rgb` or `#rrggbb` to three 0-255 channels. */
function channels(colour: string): [number, number, number] {
  const digits = colour.trim().replace(/^#/, "");
  const full =
    digits.length === 3
      ? digits
          .split("")
          .map((c) => c + c)
          .join("")
      : digits;
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

/**
 * `t` is the share of `b`, so `mix(x, y, 0)` is `x` and `mix(x, y, 1)` is `y`.
 *
 * sRGB interpolation, not OKLab: these are tints of one hue over a near-neutral,
 * where the difference is not visible, and the alternative is a colour-space
 * conversion in the shipped bundle for no gain.
 */
export function mix(a: string, b: string, t: number): string {
  const [ar, ag, ab] = channels(a);
  const [br, bg, bb] = channels(b);
  const at = (x: number, y: number) => Math.max(0, Math.min(255, Math.round(x + (y - x) * t)));
  const byte = (v: number) => v.toString(16).padStart(2, "0");
  return `#${byte(at(ar, br))}${byte(at(ag, bg))}${byte(at(ab, bb))}`;
}

/** WCAG relative luminance. */
export function luminance(colour: string): number {
  const [r, g, b] = channels(colour).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio, 1 (identical) to 21 (black on white). */
export function contrast(a: string, b: string): number {
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** The colour token names resolveTheme emits, for the check and the CSS default. */
export const COLOUR_TOKENS: readonly string[] = [
  "--n-950",
  "--n-900",
  "--n-800",
  "--n-700",
  "--n-600",
  "--n-500",
  "--n-400",
  "--n-300",
  "--n-200",
  "--n-100",
  ...["accent", "danger", "warn", "ok"].flatMap((p) =>
    ["bg", "border", "text", "solid", "hover"].map((slot) => `--${p}-${slot}`),
  ),
];
