import { getSidebarWidth, setSidebarWidth } from "../api";

const WIDTH_VAR = "--sidebar-width";
const MIN_WIDTH = 180;

/** What the user chose, kept apart from what fits: a narrowed window must not rewrite it. */
let chosen: number | null = null;

export async function restoreSidebarWidth(): Promise<void> {
  chosen = await getSidebarWidth().catch(() => null);
  apply();
}

/** The ceiling follows the window, so one stored width suits a laptop and a large display. */
function clamp(width: number): number {
  const half = Math.round(document.documentElement.clientWidth / 2);
  return Math.min(Math.max(width, MIN_WIDTH), Math.max(MIN_WIDTH, half));
}

function apply(): void {
  const style = document.documentElement.style;
  if (chosen === null) style.removeProperty(WIDTH_VAR);
  else style.setProperty(WIDTH_VAR, `${clamp(chosen)}px`);
}

window.addEventListener("resize", apply);

export function resizeHandle(): HTMLElement {
  const handle = document.createElement("div");
  handle.className = "sidebar-resize";
  handle.setAttribute("aria-hidden", "true");
  handle.addEventListener("pointerdown", (event) => onGrab(handle, event));
  return handle;
}

function onGrab(handle: HTMLElement, event: PointerEvent): void {
  const sidebar = handle.parentElement;
  if (event.button !== 0 || !sidebar) return;
  event.preventDefault();

  const box = sidebar.getBoundingClientRect();
  // Where inside the strip it was grabbed, so the edge does not jump to the pointer.
  const grab = event.clientX - box.right;
  let dragged = false;

  handle.setPointerCapture(event.pointerId);
  handle.classList.add("is-dragging");
  document.documentElement.classList.add("is-resizing");

  handle.addEventListener("pointermove", onMove);
  handle.addEventListener("pointerup", onRelease);
  handle.addEventListener("pointercancel", onRelease);

  function onMove(move: PointerEvent): void {
    dragged = true;
    chosen = clamp(Math.round(move.clientX - grab - box.left));
    apply();
  }

  function onRelease(): void {
    handle.removeEventListener("pointermove", onMove);
    handle.removeEventListener("pointerup", onRelease);
    handle.removeEventListener("pointercancel", onRelease);
    handle.classList.remove("is-dragging");
    document.documentElement.classList.remove("is-resizing");
    // A width nobody can see is not worth an error: the drag already showed the result.
    if (dragged && chosen !== null) void setSidebarWidth(chosen).catch(() => {});
  }
}
