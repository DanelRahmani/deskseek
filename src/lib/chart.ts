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
/** More than this and a slice is a hairline. A pie is a picture of a few parts. */
export const MAX_SLICES = 8;

export interface ChartSeries {
  name: string;
  values: number[];
}

export interface ChartSpec {
  /**
   * `pie` divides one series into its labels, so it takes exactly one series, no
   * negative values and at most `MAX_SLICES` slices. `bar` and `line` plot every
   * value against the same axis instead.
   */
  type: "bar" | "line" | "pie";
  title?: string;
  labels: string[];
  series: ChartSeries[];
}

export type ChartResult = { svg: string } | { error: string };

/**
 * A window's shape as a bar chart: turns per local day, oldest first.
 *
 * `null` rather than a chart of zeros when there is nothing to draw. An empty
 * window has no shape, and a row of zero-height bars reads as "you did nothing"
 * when it means "this is the wrong window" — so the caller shows a sentence
 * instead, and *that* decision is what lives here.
 *
 * Oldest first, because a week reads left to right; the query returns newest
 * first, which is right for a list of costs and wrong for a picture of a week.
 *
 * It sits beside the renderer rather than in the panel so the nothing-to-draw
 * rule is checked by the same file that draws the bars.
 */
export function recapSpec(recap: {
  turns: number;
  days: { day: string; turns: number }[];
}): ChartSpec | null {
  if (recap.turns <= 0 || recap.days.length === 0) {
    return null;
  }
  const days = [...recap.days].reverse();
  return {
    type: "bar",
    title: "Turns per day",
    // `MM-DD` from the `YYYY-MM-DD` the database wrote: twenty full dates would
    // not fit an axis, and the year is the same for every one of them.
    labels: days.map((d) => d.day.slice(5)),
    series: [{ name: "turns", values: days.map((d) => d.turns) }],
  };
}
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
  if (type !== "bar" && type !== "line" && type !== "pie") {
    return `"type" must be "bar", "line" or "pie", not ${JSON.stringify(type ?? null)}`;
  }

  const rawLabels = spec.labels;
  if (!Array.isArray(rawLabels) || rawLabels.length === 0) {
    return '"labels" must be a non-empty array — one entry per column or point';
  }
  // A pie is a picture of a few parts, so its cap is the number of slices rather
  // than the number of columns: twenty slices are a hairline each.
  if (type === "pie" && rawLabels.length > MAX_SLICES) {
    return `"labels" has ${rawLabels.length} slices; ${MAX_SLICES} is the most a pie can show — group the small ones together first`;
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

  if (type === "pie") {
    // The rules that make a bar chart flexible are what make a pie meaningless:
    // a second series would have to be a second circle, and a slice of a whole
    // cannot have a negative area.
    if (series.length > 1) {
      return `a pie takes one series of values, not ${series.length} — its labels are the slices`;
    }
    if (series[0].values.some((value) => value < 0)) {
      return "a pie cannot show a negative value — a slice of a whole cannot be below zero";
    }
  }

  return { type, title: title || undefined, labels, series };
}

/** Point on the circle at `angle` radians, clockwise from twelve o'clock. */
function arcPoint(cx: number, cy: number, r: number, angle: number) {
  return { x: cx + r * Math.sin(angle), y: cy - r * Math.cos(angle) };
}

/**
 * The swatch-and-name row above the plot.
 *
 * `entries` are `[colour, text]`, given ready-made because a pie's entries are
 * its slices rather than its series — the one difference between the two legends.
 */
function legendRows(entries: [string, string][], y: number): string[] {
  // ponytail: the spacing estimates text width at 6.6px per character for 11.5px
  // system-ui. A very long entry overruns the next swatch; measure with
  // getComputedTextLength if that ever becomes worth it.
  const parts: string[] = [];
  let cursor = PAD_LEFT;
  entries.forEach(([color, text]) => {
    parts.push(
      `<circle cx="${(cursor + 5).toFixed(1)}" cy="${y - 4}" r="5" fill="${color}"/>`,
      `<text x="${(cursor + 16).toFixed(1)}" y="${y}" font-size="11.5" fill="${MUTED}">${esc(text)}</text>`,
    );
    cursor += 16 + text.length * 6.6 + 22;
  });
  return parts;
}

/**
 * A pie: `labels` are the slices, `series[0].values` their sizes.
 *
 * Every angle comes from the same sum the legend prints its percentages from, so
 * the numbers and the picture cannot disagree. Two degenerate cases have to be
 * spelled out because a path cannot draw either: one non-zero slice is the whole
 * circle (an arc from a point back to itself is nothing), and nothing at all is
 * a ring rather than an empty frame.
 */
function pie(
  spec: ChartSpec,
  plotTop: number,
  plotW: number,
  plotH: number,
  legendY: number,
): string[] {
  const values = spec.series[0].values;
  const total = values.reduce((sum, value) => sum + value, 0);
  const cx = CHART_WIDTH / 2;
  const cy = plotTop + plotH / 2;
  const r = Math.min(plotW, plotH) / 2 - 16;
  const share = (value: number) => (total > 0 ? value / total : 0);
  const pct = (value: number) => (share(value) * 100).toFixed(1);

  const parts: string[] = [];
  const positive = values.filter((value) => value > 0).length;
  if (positive === 1) {
    const i = values.findIndex((value) => value > 0);
    parts.push(
      `<circle cx="${cx}" cy="${cy}" r="${r.toFixed(1)}" fill="${COLORS[i % COLORS.length]}"><title>${esc(`${spec.labels[i]}: ${values[i]} (100%)`)}</title></circle>`,
    );
  } else if (positive > 1) {
    let from = 0;
    values.forEach((value, i) => {
      if (value <= 0) return;
      const to = from + share(value) * Math.PI * 2;
      const a = arcPoint(cx, cy, r, from);
      const b = arcPoint(cx, cy, r, to);
      // The flag is what tells the renderer to take the long way round; without
      // it a slice past half the circle is drawn as its complement.
      const large = to - from > Math.PI ? 1 : 0;
      parts.push(
        `<path d="M ${cx} ${cy} L ${a.x.toFixed(1)} ${a.y.toFixed(1)} A ${r.toFixed(1)} ${r.toFixed(1)} 0 ${large} 1 ${b.x.toFixed(1)} ${b.y.toFixed(1)} Z" fill="${COLORS[i % COLORS.length]}"><title>${esc(`${spec.labels[i]}: ${value} (${pct(value)}%)`)}</title></path>`,
      );
      from = to;
    });
  } else {
    parts.push(
      `<circle cx="${cx}" cy="${cy}" r="${r.toFixed(1)}" fill="none" stroke="${GRID}" stroke-width="14"/>`,
    );
  }

  parts.push(
    ...legendRows(
      spec.labels.map((label, i) => [
        COLORS[i % COLORS.length],
        `${label} — ${values[i]} (${pct(values[i])}%)`,
      ]),
      legendY,
    ),
  );
  return parts;
}

/** The picture. Assumes a spec that already passed `parse`. */
function draw(spec: ChartSpec): string {
  const legendY = spec.title ? 46 : 24;
  const plotTop = legendY + 20;
  const plotW = CHART_WIDTH - PAD_LEFT - PAD_RIGHT;
  const plotH = CHART_HEIGHT - plotTop - PAD_BOTTOM;

  const parts: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${CHART_WIDTH} ${CHART_HEIGHT}" width="${CHART_WIDTH}" height="${CHART_HEIGHT}" role="img" font-family="system-ui, sans-serif">`,
    `<title>${esc(spec.title || `${spec.type === "pie" ? spec.labels.join(", ") : spec.series.map((s) => s.name).join(", ")} — ${spec.type} chart`)}</title>`,
  ];
  if (spec.title) {
    parts.push(
      `<text x="${CHART_WIDTH / 2}" y="24" text-anchor="middle" font-size="15" font-weight="600" fill="${INK}">${esc(spec.title)}</text>`,
    );
  }

  if (spec.type === "pie") {
    parts.push(...pie(spec, plotTop, plotW, plotH, legendY), "</svg>");
    return parts.join("");
  }

  const all = spec.series.flatMap((s) => s.values);
  const low = Math.min(0, ...all);
  const high = Math.max(0, ...all);
  const scale = niceTicks(low, high, 5);
  // The axis spans the ticks and the data, so no point can fall off the chart.
  const axisMin = Math.min(low, scale.values[0] ?? low);
  const axisMax = Math.max(high, scale.values[scale.values.length - 1] ?? high);

  const y = (value: number) => plotTop + (plotH * (axisMax - value)) / (axisMax - axisMin);
  const zero = y(0);
  const count = spec.labels.length;
  const groupW = plotW / count;
  const centre = (i: number) => PAD_LEFT + i * groupW + groupW / 2;

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

  parts.push(
    ...legendRows(
      spec.series.map((s, i) => [COLORS[i % COLORS.length], s.name]),
      legendY,
    ),
  );

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
