import { t } from "./strings";

export type UpdateState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "upToDate"; version: string }
  | { phase: "available"; version: string }
  | { phase: "installing"; version: string }
  | { phase: "installed"; version: string }
  | { phase: "failed"; step: "check" | "install"; detail: string };

export interface UpdatesOptions {
  version: string;
  state: UpdateState;
  onCheck: () => void;
  onUpdate: () => void;
}

export function renderUpdates(options: UpdatesOptions): HTMLElement {
  const pane = document.createElement("div");
  pane.className = "settings-pane";

  const heading = document.createElement("h1");
  heading.textContent = t.updates.heading;

  const version = document.createElement("p");
  version.className = "settings-value";
  version.textContent = t.updates.version(options.version);

  const checking = options.state.phase === "checking";
  const check = button("check-updates", checking ? t.updates.checking : t.updates.check);
  check.disabled = checking || options.state.phase === "installing";
  check.addEventListener("click", options.onCheck);

  pane.append(heading, version, check);
  for (const line of outcome(options)) pane.append(line);

  return pane;
}

function outcome(options: UpdatesOptions): HTMLElement[] {
  const state = options.state;
  switch (state.phase) {
    case "upToDate":
      return [status(t.updates.upToDate(state.version), "is-ok")];
    case "available":
    case "installing":
      return [status(t.updates.available(state.version)), update(options, state.phase)];
    case "installed":
      return [status(t.updates.installed(state.version)), helper(t.updates.installedHint)];
    case "failed":
      return [
        status(state.step === "check" ? t.updates.checkFailed : t.updates.installFailed, "is-failed"),
        helper(state.detail),
      ];
    default:
      return [];
  }
}

function update(options: UpdatesOptions, phase: "available" | "installing"): HTMLElement {
  const busy = phase === "installing";
  const element = button("install-update", busy ? t.updates.installing : t.updates.update);
  element.disabled = busy;
  element.addEventListener("click", options.onUpdate);
  return element;
}

function button(focusKey: string, label: string): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "button primary";
  element.dataset.focusKey = focusKey;
  element.textContent = label;
  return element;
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
