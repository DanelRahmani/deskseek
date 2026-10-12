/**
 * Checks for the chart artifact in chart.ts. Plain Node, no test framework:
 *
 *   npm run check
 *
 * The spec comes from the model, so what is asserted here is the contract it
 * has to satisfy — the value-to-pixel mapping, the escaping, and the messages
 * that teach it what it sent wrong. Nothing here hand-computes a pixel: the
 * assertions are ordering and containment, so they fail when the logic breaks
 * rather than when the layout is tuned.
 */
import { chartSvg, CHART_HEIGHT, CHART_WIDTH, MAX_POINTS } from "./chart.ts";

function ok(condition: boolean, message: string): void {
  if (!condition) throw new Error(`FAIL: ${message}`);
}

/** The SVG, or a failure naming the message the model would have received. */
function svgOf(text: string): string {
  const result = chartSvg(text);
  if ("error" in result) throw new Error(`FAIL: expected a chart, got: ${result.error}`);
  return result.svg;
}

/** The complaint, or a failure saying a chart came back instead. */
function errorOf(text: string): string {
  const result = chartSvg(text);
  if (!("error" in result)) throw new Error("FAIL: expected an error, got a chart");
  return result.error;
}

/** The bar rectangles as [x, y, width, height] — the svg element's own size is not one. */
function bars(svg: string): number[][] {
  return [
    ...svg.matchAll(/x="(-?[\d.]+)" y="(-?[\d.]+)" width="([\d.]+)" height="([\d.]+)"/g),
  ].map((m) => [Number(m[1]), Number(m[2]), Number(m[3]), Number(m[4])]);
}

/**
 * Nothing may be drawn outside the frame. A mis-scaled axis puts a bar off the
 * top or through the floor, and that is invisible until someone looks at
 * exactly the case that triggers it.
 */
function insideFrame(rects: number[][], what: string): void {
  for (const [x, y, w, h] of rects) {
    ok(
      x >= 0 && y >= 0 && x + w <= CHART_WIDTH && y + h <= CHART_HEIGHT,
      `${what}: a bar landed outside the chart at (${x}, ${y}, ${w}, ${h})`,
    );
  }
}

// A bar chart draws one rect per value, names the series, and labels the axis.
const bars1 = svgOf(
  JSON.stringify({
    type: "bar",
    title: "Revenue",
    labels: ["Jan", "Feb"],
    series: [{ name: "Sales", values: [10, 40] }],
  }),
);
ok(bars1.includes("Sales"), "the series name belongs in the legend");
ok(bars1.includes(">10<") || bars1.includes(">40<"), "the values should reach the axis");
ok(bars1.includes(">Jan<") && bars1.includes(">Feb<"), "every label should be drawn");
ok(bars1.includes(">Revenue<"), "a title should be drawn");
ok(bars1.includes('role="img"') && bars1.includes("<title>"), "a chart needs a name for a reader");

// The one thing a chart has to get right: bigger draws taller.
const heights = bars(bars1).map((r) => r[3]);
ok(heights.length === 2, `two bars expected, got ${heights.length}`);
ok(heights[1] > heights[0], "the larger value must draw the taller bar");
insideFrame(bars(bars1), "bar chart");

// A line chart draws a polyline with a point per value.
const line = svgOf(
  JSON.stringify({ type: "line", labels: ["Q1", "Q2", "Q3"], series: [{ name: "Sales", values: [3, 5, 4] }] }),
);
ok(line.includes("<polyline"), "a line chart needs a polyline");
// The polyline's own point count, not the circle count: the legend draws one
// swatch per series and those are circles too.
const points = /points="([^"]*)"/.exec(line)?.[1].split(" ") ?? [];
ok(points.length === 3, `one point per label, got ${points.length}`);
ok(!line.includes("<rect"), "a line chart draws no bars");

// Two series are drawn twice, in two colours, with both names listed.
const grouped = svgOf(
  JSON.stringify({
    type: "bar",
    labels: ["A", "B"],
    series: [
      { name: "Costs", values: [5, 6] },
      { name: "Profit", values: [9, 2] },
    ],
  }),
);
ok(bars(grouped).length === 4, "two series over two labels is four bars");
ok(grouped.includes("Costs") && grouped.includes("Profit"), "both series must be named");
insideFrame(bars(grouped), "grouped chart");

// Labels and titles come from the model, so they must never become markup.
const hostile = svgOf(
  JSON.stringify({
    type: "bar",
    title: "<script>alert(1)</script>",
    labels: ["</text><img src=x onerror=1>"],
    series: [{ name: "<b>bold</b>", values: [1] }],
  }),
);
ok(!hostile.includes("<script"), "a title must not become markup");
ok(!hostile.includes("<img"), "a label must not become markup");
ok(!hostile.includes("<b>"), "a series name must not become markup");
ok(hostile.includes("&lt;script&gt;"), "the title should still be readable, escaped");

// Degenerate data must still produce drawable coordinates, not NaN.
const flat = svgOf(JSON.stringify({ type: "bar", labels: ["a", "b"], series: [{ name: "n", values: [0, 0] }] }));
ok(!/NaN|Infinity/.test(flat), "an all-zero chart must not produce NaN");
ok(bars(flat).every((r) => r[3] >= 1), "a zero bar is a hairline, not nothing");
insideFrame(bars(flat), "all-zero chart");

// Negative values hang below the zero line and the axis still shows zero.
const losses = svgOf(
  JSON.stringify({ type: "bar", labels: ["loss", "profit"], series: [{ name: "P&L", values: [-30, 20] }] }),
);
ok(!/NaN|Infinity/.test(losses), "negative values must not produce NaN");
ok(bars(losses).length === 2, "a negative bar is still drawn");
ok(losses.includes(">0<"), "the zero line must be labelled");
ok(losses.includes("P&amp;L") || losses.includes("P&L"), "an ampersand in a name must survive");
insideFrame(bars(losses), "chart with negatives");
// A negative bar hangs under the zero line and a positive one above it.
const [lossBar, profitBar] = bars(losses);
ok(lossBar[1] + lossBar[3] > profitBar[1] + profitBar[3], "a negative bar belongs lower than a positive one");

// Every wrong shape the model actually sends gets a message that names it.
ok(errorOf("this is not json").includes("JSON"), "bad JSON must say so");
ok(errorOf('"12"').includes("object"), "a bare value is not a spec");
ok(errorOf("[]").includes("object"), "an array is not a spec");
ok(errorOf(JSON.stringify({ labels: ["a"], series: [{ values: [1] }] })).includes("type"), "type is required");
ok(
  errorOf(JSON.stringify({ type: "pie", labels: ["a"], series: [{ values: [1] }] })).includes("pie"),
  "the error should quote what was sent",
);
ok(
  errorOf(JSON.stringify({ type: "bar", labels: ["a", "b"], series: [{ values: [1] }] })).includes(
    "one number per label",
  ),
  "a length mismatch must be explained",
);
ok(
  errorOf(JSON.stringify({ type: "bar", labels: ["a"], series: [{ values: ["x"] }] })).includes("numbers"),
  "a non-number value must be rejected",
);
ok(
  errorOf(JSON.stringify({ type: "bar", labels: [], series: [{ values: [] }] })).includes("non-empty"),
  "an empty chart must be rejected",
);
const tooMany = Array.from({ length: MAX_POINTS + 1 }, (_, i) => `l${i}`);
ok(
  errorOf(JSON.stringify({ type: "bar", labels: tooMany, series: [{ values: tooMany.map(() => 1) }] })).includes(
    String(MAX_POINTS),
  ),
  "too many points must name the cap",
);

console.log("chart.check.ts: all checks passed");
