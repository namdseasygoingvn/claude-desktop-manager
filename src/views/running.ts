/**
 * The live-state label, shared by a sidebar row and the detail line. It stays an inline span in
 * both: the wave paints a gradient through the glyphs, and an inline box is sized by its text,
 * so the highlight crosses the word itself. A grid or block box would stretch to its column and
 * sweep mostly empty space.
 */
export function runningLabel(text: string): HTMLElement {
  const span = document.createElement("span");
  span.className = "is-running";
  span.textContent = text;
  return span;
}

/** Same wave, cyan instead of green — see `.is-sync` in style.css. */
export function syncLabel(text: string): HTMLElement {
  const span = document.createElement("span");
  span.className = "is-sync";
  span.textContent = text;
  return span;
}
