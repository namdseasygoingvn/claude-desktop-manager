import type { ClaudeUpdateStatus } from "../api";
import { t } from "./strings";

export type ClaudeUpdateState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "ready"; status: ClaudeUpdateStatus }
  | { phase: "installing"; status: ClaudeUpdateStatus }
  | { phase: "restarting"; status: ClaudeUpdateStatus }
  | {
      phase: "failed";
      step: "check" | "install" | "restart";
      detail: string;
      status: ClaudeUpdateStatus | null;
    };

export interface ClaudeUpdateOptions {
  state: ClaudeUpdateState;
  onCheck: () => void;
  onInstall: () => void;
  onRestartOutdated: () => void;
}

export function renderClaudeUpdate(options: ClaudeUpdateOptions): HTMLElement {
  const { state } = options;
  const busy = isClaudeUpdateBusy(state);
  const known = statusOf(state);

  const pane = document.createElement("div");
  pane.className = "settings-pane";

  const heading = document.createElement("h2");
  heading.textContent = t.claudeUpdate.heading;
  pane.append(heading);

  if (known) pane.append(value(t.claudeUpdate.installed(known.installed)));

  pane.append(
    button(
      "claude-check",
      state.phase === "checking" ? t.claudeUpdate.checking : t.claudeUpdate.check,
      busy,
      options.onCheck,
    ),
  );

  if (known?.available) {
    pane.append(
      status(t.claudeUpdate.available(known.latest)),
      helper(t.claudeUpdate.closesOriginal),
      button(
        "claude-install",
        state.phase === "installing" ? t.claudeUpdate.installing : t.claudeUpdate.update,
        busy,
        options.onInstall,
      ),
    );
  }

  const outdated = known?.outdatedProfileIds.length ?? 0;
  if (outdated > 0) {
    pane.append(
      status(t.claudeUpdate.outdated(outdated)),
      button(
        "claude-restart",
        state.phase === "restarting" ? t.claudeUpdate.restarting : t.claudeUpdate.restart,
        busy,
        options.onRestartOutdated,
      ),
    );
  }

  if (known && !known.available && outdated === 0 && state.phase !== "failed") {
    pane.append(status(t.claudeUpdate.upToDate(known.installed), "is-ok"));
  }

  if (state.phase === "failed") pane.append(...failure(state));

  return pane;
}

export function isClaudeUpdateBusy(state: ClaudeUpdateState): boolean {
  return (
    state.phase === "checking" || state.phase === "installing" || state.phase === "restarting"
  );
}

function statusOf(state: ClaudeUpdateState): ClaudeUpdateStatus | null {
  switch (state.phase) {
    case "ready":
    case "installing":
    case "restarting":
    case "failed":
      return state.status;
    default:
      return null;
  }
}

function failure(state: Extract<ClaudeUpdateState, { phase: "failed" }>): HTMLElement[] {
  if (state.step === "restart") return [status(state.detail, "is-failed")];
  return [status(t.claudeUpdate.failed[state.step], "is-failed"), helper(state.detail)];
}

function button(
  focusKey: string,
  label: string,
  disabled: boolean,
  onClick: () => void,
): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "button primary";
  element.dataset.focusKey = focusKey;
  element.textContent = label;
  element.disabled = disabled;
  element.addEventListener("click", onClick);
  return element;
}

function value(text: string): HTMLElement {
  const line = document.createElement("p");
  line.className = "settings-value";
  line.textContent = text;
  return line;
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
