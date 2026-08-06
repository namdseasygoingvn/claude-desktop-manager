export type Platform = "macos" | "windows";

function detect(): Platform {
  const hint = `${navigator.userAgent} ${navigator.platform ?? ""}`;
  return /mac|iphone|ipad|ipod/i.test(hint) ? "macos" : "windows";
}

export const platform: Platform = detect();
export const isMac = platform === "macos";

export type ButtonRole = "affirmative" | "destructive" | "secondary" | "cancel";

interface Roled {
  role: ButtonRole;
}

/**
 * macOS: affirmative rightmost; for destructive alerts Cancel takes the rightmost default slot
 * and the destructive button moves left. Windows: affirmative or destructive leftmost, Cancel
 * always rightmost.
 */
export function orderButtons<T extends Roled>(buttons: T[]): T[] {
  const by = (role: ButtonRole) => buttons.filter((b) => b.role === role);
  const lead = [...by("destructive"), ...by("affirmative")];
  const secondary = by("secondary");
  const cancel = by("cancel");
  if (!isMac) return [...lead, ...secondary, ...cancel];
  const preferred = defaultButton(buttons);
  const rest = [...lead, ...cancel].filter((button) => button !== preferred);
  return preferred ? [...secondary, ...rest, preferred] : [...secondary, ...rest];
}

/** The button Return activates and that takes initial focus. Never a destructive one. */
export function defaultButton<T extends Roled>(buttons: T[]): T | undefined {
  const destructive = buttons.find((b) => b.role === "destructive");
  const cancel = buttons.find((b) => b.role === "cancel");
  if (destructive && cancel) return cancel;
  return buttons.find((b) => b.role === "affirmative") ?? cancel;
}

export interface Shortcut {
  key: string;
  meta?: boolean;
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
}

export const shortcuts = {
  newProfile: isMac ? { key: "n", meta: true } : { key: "n", ctrl: true },
  rename: isMac ? { key: "r", meta: true } : { key: "F2" },
  delete: isMac ? { key: "Backspace", meta: true } : { key: "Delete" },
  editConfig: isMac ? { key: "e", meta: true } : { key: "e", ctrl: true },
  reveal: isMac ? { key: "r", meta: true, shift: true } : { key: "r", ctrl: true, shift: true },
  hideWindow: isMac ? { key: "w", meta: true } : { key: "Escape" },
  moveUp: { key: "ArrowUp", alt: true },
  moveDown: { key: "ArrowDown", alt: true },
} satisfies Record<string, Shortcut>;

export function matches(event: KeyboardEvent, shortcut: Shortcut): boolean {
  if (event.key.toLowerCase() !== shortcut.key.toLowerCase()) return false;
  if (!!shortcut.meta !== event.metaKey) return false;
  if (!!shortcut.ctrl !== event.ctrlKey) return false;
  if (!!shortcut.shift !== event.shiftKey) return false;
  if (!!shortcut.alt !== event.altKey) return false;
  return true;
}
