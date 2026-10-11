/**
 * Copying text out of the app.
 *
 * `navigator.clipboard` is the API to prefer, but it needs a secure context and
 * a permission the webview may refuse — the production origin is a custom
 * protocol, which is not guaranteed to count as one. The fallback is the
 * pre-2018 dance with a throwaway textarea, which still works everywhere and is
 * invisible: it is positioned off-screen and removed in `finally`.
 */
export async function copyText(text: string): Promise<void> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch {
    // Refused or unavailable: fall through rather than leave the click dead.
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.top = "-1000px";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  try {
    document.execCommand("copy");
  } finally {
    document.body.removeChild(area);
  }
}
