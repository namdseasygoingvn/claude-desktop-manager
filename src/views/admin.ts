import type { ViewBounds } from "../api";
import { t } from "./strings";

export interface AdminOptions {
  onBounds: (bounds: ViewBounds) => void;
  error: string | null;
  onRetry: () => void;
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

    const actions = document.createElement("div");
    actions.className = "full-pane-actions";
    const retry = document.createElement("button");
    retry.type = "button";
    retry.className = "button primary";
    retry.textContent = t.common.tryAgain;
    retry.addEventListener("click", options.onRetry);
    actions.append(retry);
    pane.append(actions);
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
