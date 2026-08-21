import { t } from "./strings";

export interface AdminOptions {
  onOpen: () => void;
  error: string | null;
}

export function renderAdmin(options: AdminOptions): HTMLElement {
  const pane = document.createElement("section");
  pane.className = "full-pane";

  const heading = document.createElement("h1");
  heading.textContent = t.admin.heading;

  const body = document.createElement("p");
  body.textContent = t.admin.body;

  const hint = document.createElement("p");
  hint.textContent = t.admin.hint;

  const primary = document.createElement("button");
  primary.type = "button";
  primary.className = "button primary large";
  primary.textContent = t.admin.open;
  primary.dataset.focusKey = "admin-open";
  primary.addEventListener("click", options.onOpen);

  pane.append(heading, body, hint, primary);

  if (options.error) {
    const error = document.createElement("p");
    error.className = "settings-status is-failed";
    error.setAttribute("role", "status");
    error.textContent = options.error;
    pane.append(error);
  }

  return pane;
}
