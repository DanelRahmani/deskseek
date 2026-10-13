/**
 * The tray, held to the window.
 *
 * The tray's menu is four rows in `src-tauri/src/tray.rs`, and half of them are
 * run by the *window*: Rust emits the id and `src/App.tsx` hands it to the same
 * action switch the command palette uses. That is only sound while those ids are
 * the palette's own ids — a rename on either side leaves a menu item that does
 * nothing at all, quietly, which is the worst way for a menu to fail.
 *
 * So this reads both sides as text and holds them to each other. It is the only
 * check here that reads Rust, because it is the only rule that spans the two
 * languages; the Rust half of the same agreement — the ids, their uniqueness,
 * and which half of the table each one is in — is asserted in `tray.rs` itself
 * by `the_tray_menu_is_the_four_things_the_window_can_do`.
 */
// @ts-ignore — see theme.check.ts: the app's tsconfig carries no Node types, and
// adding @types/node for one readFileSync would be a devDependency for nothing.
import { readFileSync } from "node:fs";

const failures: string[] = [];

function fail(message: string): void {
  failures.push(message);
}

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    throw new Error(`FAIL: ${path} could not be read — the check has nothing to hold to`);
  }
}

const rust = read("src-tauri/src/tray.rs");
const commands = read("src/lib/commands.ts");
const app = read("src/App.tsx");

// --- the table, as the Rust build sees it ------------------------------------

const rows = [...rust.matchAll(/^\s*\("([a-z-]+)", "([^"]+)", (true|false)\),\s*$/gm)].map(
  (match) => ({ id: match[1], label: match[2], toWindow: match[3] === "true" }),
);

if (rows.length === 0) {
  fail(
    "no tray menu table was found in src-tauri/src/tray.rs. If its shape changed, this check " +
      "has stopped reading it — which is a failure, not a pass",
  );
}
if (rows.length !== 4) fail(`the tray menu is ${rows.length} items, not 4`);
if (new Set(rows.map((row) => row.id)).size !== rows.length) {
  fail(`two tray menu items share an id: ${rows.map((row) => row.id).join(", ")}`);
}
for (const row of rows) {
  if (!row.label.trim()) fail(`the tray item "${row.id}" has no label to read`);
}

// --- the half the window runs must be the palette's own ids ------------------

const paletteIds = new Set(
  [...commands.matchAll(/\{\s*id:\s*"([a-z-]+)"/g)].map((match) => match[1]),
);
if (paletteIds.size === 0) {
  fail("no palette action ids were found in src/lib/commands.ts — the matching has gone stale");
}

const forwarded = rows.filter((row) => row.toWindow).map((row) => row.id);
for (const id of forwarded) {
  if (!paletteIds.has(id)) {
    fail(
      `the tray forwards "${id}" to the window, but the palette has no such action — the menu ` +
        `item would do nothing. Palette actions: ${[...paletteIds].sort().join(", ")}`,
    );
  }
}

// --- and the half Rust owns must not be the window's -------------------------

for (const id of rows.filter((row) => !row.toWindow).map((row) => row.id)) {
  if (paletteIds.has(id)) {
    // Not an error in itself, but it means the flag is wrong: Rust would never
    // ask, and the item would be listed as something the window does.
    fail(`"${id}" is this file's own action but is also a palette id — check the table's flag`);
  }
}

// --- the event the window listens for is the event Rust sends ----------------

const event = /\bEVENT_TRAY\s*:\s*&str\s*=\s*"([^"]+)"/.exec(rust)?.[1];
if (!event) {
  fail("src-tauri/src/tray.rs no longer names an event constant for the window to listen for");
} else {
  if (!app.includes(`onTray(`)) {
    fail("src/App.tsx no longer listens for the tray at all");
  }
  if (!read("src/api.ts").includes(`listen<TrayEvent>("${event}"`)) {
    fail(`src/api.ts does not listen on "${event}" — the constant and the listener have drifted`);
  }
}

if (failures.length > 0) {
  throw new Error(`FAIL: ${failures.length} tray problem(s):\n  ${failures.join("\n  ")}`);
}

console.log(
  `tray.check.ts: all checks passed (${rows.length} menu items, ${forwarded.length} routed to ` +
    `the window and both of them palette actions)`,
);
