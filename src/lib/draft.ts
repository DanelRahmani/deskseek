/**
 * The composer's half-typed text, kept outside the React tree.
 *
 * A render throw unmounts everything under the error boundary and takes the
 * draft with it — the boundary cannot see state that lives in the tree it
 * replaces. Mirroring the text into `sessionStorage` is the one-trick fix: it
 * survives the boundary's *Try again* and a full window reload, and it is
 * deliberately session-scoped, because a draft is for this run of the app, not
 * something to hand back a week later.
 *
 * Keyed by chat so two panes do not overwrite each other. The chatless
 * composer (no chat yet) shares one `new` key, which is the same conversation
 * as far as the composer is concerned.
 */
export const draftKeyFor = (chatId: string | null | undefined): string =>
  `deskseek.draft:${chatId ?? "new"}`;

/** Tolerant read: an unavailable store must not take the composer down. */
export function loadDraft(chatId: string | null | undefined): string {
  try {
    return sessionStorage.getItem(draftKeyFor(chatId)) ?? "";
  } catch {
    return "";
  }
}

export function saveDraft(chatId: string | null | undefined, text: string): void {
  try {
    if (text) sessionStorage.setItem(draftKeyFor(chatId), text);
    else sessionStorage.removeItem(draftKeyFor(chatId));
  } catch {
    // A full or unavailable sessionStorage must not break typing.
  }
}
