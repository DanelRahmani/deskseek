/**
 * The app's icon vocabulary, drawn here rather than pulled from a package.
 *
 * Seven glyphs is not worth a dependency, and drawing them keeps one style
 * across the app: 24px grid, 1.75 stroke, no fill, currentColor. The point is
 * consistency — a copy button, a retry, a disclosure and a close should not be
 * four unrelated marks that happen to sit on the same surface.
 */
import type { MouseEvent } from "react";

export type IconName =
  | "copy"
  | "check"
  | "retry"
  | "edit"
  | "chevron"
  | "archive"
  | "restore"
  | "split"
  | "close";

const PATHS: Record<IconName, string> = {
  // Two offset rounded rectangles: the standard "copy" shape.
  copy: "M9.5 9.5h8a1.5 1.5 0 0 1 1.5 1.5v8a1.5 1.5 0 0 1-1.5 1.5h-8A1.5 1.5 0 0 1 8 19v-8a1.5 1.5 0 0 1 1.5-1.5ZM5.5 15H5a1.5 1.5 0 0 1-1.5-1.5v-8A1.5 1.5 0 0 1 5 4h8A1.5 1.5 0 0 1 14.5 5.5V6",
  check: "M4.5 12.5 9.5 17.5 19.5 6.5",
  // A circular arrow, open at the top right.
  retry: "M20 12a8 8 0 1 1-2.4-5.7M20 3.5V9h-5.5",
  edit: "M4 20h4.2l10.3-10.3a2.1 2.1 0 0 0-3-3L5.2 17V20ZM14.2 5.5l4.3 4.3",
  chevron: "M9.5 5.5 16 12l-6.5 6.5",
  archive: "M3.5 4.5h17v4h-17v-4ZM5.5 8.5v10a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1v-10M10 13h4",
  restore: "M12 15.5V4M8 8l4-4 4 4M4.5 15v3a1 1 0 0 0 1 1h13a1 1 0 0 0 1-1v-3",
  split: "M4 4.5h16a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-13a1 1 0 0 1 1-1ZM12 4.5v15",
  close: "M6.5 6.5l11 11M17.5 6.5l-11 11",
};

export function Icon({
  name,
  size = 14,
  className,
}: {
  name: IconName;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`block ${className ?? ""}`}
      aria-hidden
      focusable="false"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}

/**
 * A quiet icon button: no border, no label, a background only once you point at
 * it. Used wherever the app used to put a bordered box with a word in it.
 *
 * `label` is both the tooltip and the accessible name — the icon on its own
 * says nothing to a screen reader.
 */
export function IconButton({
  icon,
  label,
  onClick,
  disabled,
  tone = "plain",
  className = "",
}: {
  icon: IconName;
  label: string;
  onClick: (e: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  tone?: "plain" | "ok" | "danger";
  className?: string;
}) {
  const colour =
    tone === "ok"
      ? "text-emerald-400 hover:text-emerald-300"
      : tone === "danger"
        ? "text-neutral-500 hover:bg-red-950/40 hover:text-red-300"
        : "text-neutral-500 hover:bg-neutral-800 hover:text-neutral-200";
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={label}
      aria-label={label}
      className={`grid h-6 w-6 shrink-0 place-items-center rounded-md transition duration-150 disabled:cursor-not-allowed disabled:opacity-40 ${colour} ${className}`}
    >
      <Icon name={icon} />
    </button>
  );
}
