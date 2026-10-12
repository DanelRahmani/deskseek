/**
 * Timestamps, formatted the one way the app formats them.
 *
 * A unix-seconds stamp (what SQLite and the events carry) as a local, readable
 * moment. Its own module because three surfaces now show one — the Scheduled
 * panel, the approval log — and a second copy of a date format is how two lists
 * come to disagree about what "Mon 12 Oct 14:03" means.
 */
export function when(ts: number): string {
  return new Date(ts * 1000).toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
