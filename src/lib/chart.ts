/**
 * Charts as data.
 *
 * A `chart` artifact is not code: the model sends a small JSON spec and this
 * module turns it into the SVG the panel draws. Pure string work, so the
 * mapping from value to pixel — the part that is easy to get subtly wrong and
 * impossible to check by looking at it — is pinned by chart.check.ts.
 *
 * `chartSvg` never throws: a spec the model got wrong comes back as a message
 * to show it, the same way `parse_artifact` and `parse_tasks` answer in Rust.
 */

/** More columns than this is a table, not a chart. */
export const MAX_POINTS = 200;
/** More than this and the bars are thinner than the gaps. */
export const MAX_SERIES = 8;

export interface ChartSeries {
  name: string;
  values: number[];
}

export interface ChartSpec {
  type: "bar" | "line";
  title?: string;
  labels: string[];
  series: ChartSeries[];
}

export type ChartResult = { svg: string } | { error: string };

// A fixed palette rather than the theme's tokens: the frame draws on white, so
// the colours have to be chosen against white and cannot follow the app theme.
const COLORS = ["#2f6fd0", "#d0562f", "#2f9e6a", "#8a5cd0", "#a8761f", "#3f8fa8"];

/** The frame every chart is drawn in. Exported so the check can assert nothing lands outside it. */
export const CHART_WIDTH = 720;
export const CHART_HEIGHT = 420;
const PAD_LEFT = 64;
const PAD_RIGHT = 24;
const PAD_BOTTOM = 54;

const INK = "#11181c";
const MUTED = "#4b5358";
const AXIS = "#9aa0a6";
const GRID = "#e6e8ea";

/** XML text, so a label can never become markup. `&` has to go first. */
function esc(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/**
 * Round ticks over the data's range. The step is always 1, 2 or 5 times a power
 * of ten, which is what makes an axis readable; `decimals` is derived from that
 * step so the labels never show float noise.
 */
function niceTicks(low: number, high: number, count: number): { values: number[]; decimals: number } {
  // A flat series still has to be drawn against something, so the range the
  // ticks span is never zero-width — and it is `top`, not `high`, that bounds
  // the loop, or a series of zeros would produce a single tick and a division
  // by a zero-width axis.
  const span = high - low || 1;
  const top = low + span;
  const raw = span / count;
  const magnitude = Math.pow(10, Math.floor(Math.log10(raw)));
  const normalised = raw / magnitude;
  const step = (normalised <= 1 ? 1 : normalised <= 2 ? 2 : normalised <= 5 ? 5 : 10) * magnitude;
  const start = Math.ceil(low / step) * step;

  const values: number[] = [];
  // Push first, then stop: the last tick is the first one at or past the top, so
  // the axis ends on a round number with the data's peak just below it. Bounded
  // by `count + 2`, so a step that came out wrong can never spin here.
  for (let i = 0; i <= count + 2; i++) {
    const value = start + i * step;
    values.push(value);
    if (value >= top - step * 1e-9) break;
  }
  const decimals = Math.max(0, Math.min(6, -Math.floor(Math.log10(step) + 1e-9)));
  return { values, decimals };
}

/**
 * The spec, or the reason it is not one. Errors name what was sent and what is
 * expected, because the text goes back to the model as a tool result.
 */
function parse(text: string): ChartSpec | string {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (e) {
    return `content is not valid JSON (${e instanceof Error ? e.message : String(e)})`;
  }
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return 'content must be a JSON object like {"type":"bar","labels":[...],"series":[{"name":"...","values":[...]}]}';
  }
  const spec = raw as Record<string, unknown>;

  const type = spec.type;
  if (type !== "bar" && type !== "line") {
    return `"type" must be "bar" or "line", not ${JSON.stringify(type ?? null)}`;
  }

  const rawLabels = spec.labels;
  if (!Array.isArray(rawLabels) || rawLabels.length === 0) {
    return '"labels" must be a non-empty array — one entry per column or point';
  }
  if (rawLabels.length > MAX_POINTS) {
    return `"labels" has ${rawLabels.length} entries; ${MAX_POINTS} is the most a chart can show — group or aggregate the data first`;
  }
  const labels: string[] = [];
  for (const label of rawLabels) {
    if (typeof label === "string") labels.push(label);
    else if (typeof label === "number" && Number.isFinite(label)) labels.push(String(label));
    else return 'every entry in "labels" must be a string or a number';
  }

  const rawSeries = spec.series;
  if (!Array.isArray(rawSeries) || rawSeries.length === 0) {
    return '"series" must be a non-empty array of {"name":..., "values":[...]}';
  }
  if (rawSeries.length > MAX_SERIES) {
    return `"series" has ${rawSeries.length} entries; ${MAX_SERIES} is the most that stays readable`;
  }
  const series: ChartSeries[] = [];
  for (let i = 0; i < rawSeries.length; i++) {
    const entry = rawSeries[i];
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
      return `series[${i}] must be an object with "values"`;
    }
    const { name, values } = entry as { name?: unknown; values?: unknown };
    if (!Array.isArray(values) || values.length !== labels.length) {
      const got = Array.isArray(values) ? `${values.length} values` : "none";
      return `series[${i}].values must have one number per label (${labels.length}), not ${got}`;
    }
    for (const value of values) {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        return `series[${i}].values must be numbers`;
      }
    }
    if (name !== undefined && typeof name !== "string") return `series[${i}].name must be a string`;
    series.push({ name: name || `Series ${i + 1}`, values: values as number[] });
  }

  const title = spec.title;
  if (title !== undefined && typeof title !== "string") return '"title" must be a string';
  return { type, title: title || undefined, labels, series };
}

/** The picture. Assumes a spec that already passed `parse`. */
function draw(spec: ChartSpec): string {
  const all = spec.series.flatMap((s) => s.values);
  const low = Math.min(0, ...all);
  const high = Math.max(0, ...all);
  const scale = niceTicks(low, high, 5);
  // The axis spans the ticks and the data, so no point can fall off the chart.
  const axisMin = Math.min(low, scale.values[0] ?? low);
  const axisMax = Math.max(high, scale.values[scale.values.length - 1] ?? high);

  const legendY = spec.title ? 46 : 24;
  const plotTop = legendY + 20;
  const plotW = CHART_WIDTH - PAD_LEFT - PAD_RIGHT;
  const plotH = CHART_HEIGHT - plotTop - PAD_BOTTOM;

  const y = (value: number) => plotTop + (plotH * (axisMax - value)) / (axisMax - axisMin);
  const zero = y(0);
  const count = spec.labels.length;
  const groupW = plotW / count;
  const centre = (i: number) => PAD_LEFT + i * groupW + groupW / 2;

  const parts: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${CHART_WIDTH} ${CHART_HEIGHT}" width="${CHART_WIDTH}" height="${CHART_HEIGHT}" role="img" font-family="system-ui, sans-serif">`,
    `<title>${esc(spec.title || `${spec.series.map((s) => s.name).join(", ")} — ${spec.type} chart`)}</title>`,
  ];

  for (const tick of scale.values) {
    if (tick < axisMin - 1e-9 || tick > axisMax + 1e-9) continue;
    const ty = y(tick);
    parts.push(
      `<line x1="${PAD_LEFT}" y1="${ty.toFixed(1)}" x2="${PAD_LEFT + plotW}" y2="${ty.toFixed(1)}" stroke="${GRID}" stroke-width="1"/>`,
      `<text x="${PAD_LEFT - 8}" y="${(ty + 3.5).toFixed(1)}" text-anchor="end" font-size="11" fill="${AXIS}">${esc(tick.toFixed(scale.decimals))}</text>`,
    );
  }
  parts.push(
    `<line x1="${PAD_LEFT}" y1="${plotTop}" x2="${PAD_LEFT}" y2="${plotTop + plotH}" stroke="${AXIS}" stroke-width="1"/>`,
    `<line x1="${PAD_LEFT}" y1="${plotTop + plotH}" x2="${PAD_LEFT + plotW}" y2="${plotTop + plotH}" stroke="${AXIS}" stroke-width="1"/>`,
  );

  // ponytail: the legend's spacing estimates text width at 6.6px per character
  // for 11.5px system-ui. A very long series name overruns the next swatch;
  // measure with getComputedTextLength if that ever becomes worth it.
  let cursor = PAD_LEFT;
  spec.series.forEach((s, i) => {
    parts.push(
      `<circle cx="${(cursor + 5).toFixed(1)}" cy="${legendY - 4}" r="5" fill="${COLORS[i % COLORS.length]}"/>`,
      `<text x="${(cursor + 16).toFixed(1)}" y="${legendY}" font-size="11.5" fill="${MUTED}">${esc(s.name)}</text>`,
    );
    cursor += 16 + s.name.length * 6.6 + 22;
  });

  if (spec.title) {
    parts.push(
      `<text x="${CHART_WIDTH / 2}" y="24" text-anchor="middle" font-size="15" font-weight="600" fill="${INK}">${esc(spec.title)}</text>`,
    );
  }

  if (spec.type === "bar") {
    const inner = groupW * 0.72;
    const slot = inner / spec.series.length;
    const barW = Math.max(1, slot - 1);
    spec.series.forEach((s, si) => {
      const fill = COLORS[si % COLORS.length];
      s.values.forEach((value, i) => {
        const x = PAD_LEFT + i * groupW + (groupW - inner) / 2 + si * slot;
        const vy = y(value);
        parts.push(
          `<rect x="${x.toFixed(1)}" y="${(value >= 0 ? vy : zero).toFixed(1)}" width="${barW.toFixed(1)}" height="${Math.max(1, Math.abs(vy - zero)).toFixed(1)}" fill="${fill}"><title>${esc(`${spec.labels[i]} · ${s.name}: ${value}`)}</title></rect>`,
        );
      });
    });
  } else {
    // A point marker per value only while there is room for one.
    const dots = count <= 40;
    spec.series.forEach((s, si) => {
      const stroke = COLORS[si % COLORS.length];
      const points = s.values.map((v, i) => `${centre(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
      parts.push(
        `<polyline points="${points}" fill="none" stroke="${stroke}" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>`,
      );
      if (dots) {
        s.values.forEach((value, i) => {
          parts.push(
            `<circle cx="${centre(i).toFixed(1)}" cy="${y(value).toFixed(1)}" r="2.6" fill="${stroke}"><title>${esc(`${spec.labels[i]} · ${s.name}: ${value}`)}</title></circle>`,
          );
        });
      }
    });
  }

  // Every nth label, so a 40-column chart does not print 40 overlapping words.
  const every = Math.ceil(count / 12);
  spec.labels.forEach((label, i) => {
    if (i % every !== 0) return;
    const short = label.length > 11 ? `${label.slice(0, 10)}…` : label;
    const hover = short === label ? "" : `<title>${esc(label)}</title>`;
    parts.push(
      `<text x="${centre(i).toFixed(1)}" y="${plotTop + plotH + 18}" text-anchor="middle" font-size="11" fill="${MUTED}">${esc(short)}${hover}</text>`,
    );
  });

  parts.push("</svg>");
  return parts.join("");
}

/**
 * The SVG for a chart artifact's content, or why it cannot be drawn.
 *
 * Pass the result to `frameDoc("svg", svg)` to get the document the panel
 * shows — the same path a hand-written `svg` artifact takes.
 */
export function chartSvg(text: string): ChartResult {
  const spec = parse(text);
  if (typeof spec === "string") return { error: spec };
  return { svg: draw(spec) };
}
