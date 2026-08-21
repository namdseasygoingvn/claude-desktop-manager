import type { ViewBounds } from "../api";

export interface AdminOptions {
  onBounds: (bounds: ViewBounds) => void;
  error: string | null;
}

/** The members webview is a native child view the backend lays over the host's box. */
export function renderAdmin(options: AdminOptions): HTMLElement {
  const pane = document.createElement("section");
  pane.className = "admin-pane";

  if (options.error) {
    const error = document.createElement("p");
    error.className = "settings-status is-failed";
    error.setAttribute("role", "status");
    error.textContent = options.error;
    pane.append(error);
  }

  const host = document.createElement("div");
  host.className = "admin-embed";
  const observer = new ResizeObserver(() => {
    const rect = host.getBoundingClientRect();
    // A hidden or detached host measures 0×0; acting on that would race the tab-switch hide.
    if (rect.width === 0 || rect.height === 0) return;
    options.onBounds({ x: rect.x, y: rect.y, width: rect.width, height: rect.height });
  });
  observer.observe(host);
  pane.append(host);

  return pane;
}
