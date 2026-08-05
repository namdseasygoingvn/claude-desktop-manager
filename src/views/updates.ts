import { t } from "./strings";

export type UpdateState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "upToDate"; version: string }
  | { phase: "installed"; version: string }
  | { phase: "failed"; detail: string };

export interface UpdatesOptions {
  version: string;
  state: UpdateState;
  onCheck: () => void;
}

export function renderUpdates(options: UpdatesOptions): HTMLElement {
  const pane = document.createElement("div");
  pane.className = "settings-pane";

  const heading = document.createElement("h1");
  heading.textContent = t.updates.heading;

  const version = document.createElement("p");
  version.className = "settings-value";
  version.textContent = t.updates.version(options.version);

  const check = document.createElement("button");
  check.type = "button";
  check.className = "button primary";
  check.dataset.focusKey = "check-updates";
  const busy = options.state.phase === "checking";
  check.textContent = busy ? t.updates.checking : t.updates.check;
  check.disabled = busy;
  check.addEventListener("click", options.onCheck);

  pane.append(heading, version, check);
  for (const line of outcome(options.state)) pane.append(line);

  return pane;
}

function outcome(state: UpdateState): HTMLElement[] {
  switch (state.phase) {
    case "upToDate":
      return [status(t.updates.upToDate(state.version), "is-ok")];
    case "installed":
      return [status(t.updates.installed(state.version)), helper(t.updates.installedHint)];
    case "failed":
      return [status(t.updates.failed, "is-failed"), helper(state.detail)];
    default:
      return [];
  }
}

function status(text: string, tone?: "is-ok" | "is-failed"): HTMLElement {
  const line = document.createElement("p");
  line.className = tone ? `settings-status ${tone}` : "settings-status";
  line.setAttribute("role", "status");
  line.textContent = text;
  return line;
}

function helper(text: string): HTMLElement {
  const line = document.createElement("p");
  line.className = "helper";
  line.textContent = text;
  return line;
}
