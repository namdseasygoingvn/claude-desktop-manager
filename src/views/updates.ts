import { t } from "./strings";
import { formatBytes, formatDuration } from "./transfer";

/** `total` is null when the server sent no Content-Length; `speed` until the rate meter has two
 *  samples to divide. Either one missing costs a piece of the readout, not the bar. */
export interface Downloading {
  phase: "downloading";
  version: string;
  downloaded: number;
  total: number | null;
  speed: number | null;
}

export type UpdateState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "upToDate"; version: string }
  | { phase: "available"; version: string }
  | Downloading
  | { phase: "unpacking"; version: string }
  | { phase: "installed"; version: string }
  | { phase: "restarting"; version: string }
  | { phase: "failed"; step: "check" | "install" | "restart"; detail: string };

type InstallPhase = "available" | "downloading" | "unpacking";

export interface UpdatesOptions {
  version: string;
  state: UpdateState;
  onCheck: () => void;
  onUpdate: () => void;
  onRestart: () => void;
}

const METER_ATTR = "data-download-meter";

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
  check.disabled = isBusy(options.state);
  check.addEventListener("click", options.onCheck);

  pane.append(heading, version, check);
  for (const line of outcome(options)) pane.append(line);

  return pane;
}

/** Mid-operation: nothing new may be started until it settles. */
export function isBusy(state: UpdateState): boolean {
  switch (state.phase) {
    case "checking":
    case "downloading":
    case "unpacking":
    case "restarting":
      return true;
    default:
      return false;
  }
}

/** An update is waiting on the user — found, coming down, or downloaded and needing a restart. */
export function isPending(state: UpdateState): boolean {
  switch (state.phase) {
    case "available":
    case "downloading":
    case "unpacking":
    case "installed":
    case "restarting":
      return true;
    default:
      return false;
  }
}

/**
 * Progress arrives ten times a second; a full re-render would tear down the pane and move focus
 * that often. Only a phase change earns a re-render — the rest repaints the bar in place.
 */
export function paintDownload(root: ParentNode, state: Downloading): void {
  const block = root.querySelector<HTMLElement>(`[${METER_ATTR}]`);
  if (block) paint(block, state);
}

function outcome(options: UpdatesOptions): HTMLElement[] {
  const state = options.state;
  switch (state.phase) {
    case "upToDate":
      return [status(t.updates.upToDate(state.version), "is-ok")];
    case "available":
      return [status(t.updates.available(state.version)), update(options, state.phase)];
    case "downloading":
      return [
        status(t.updates.installingVersion(state.version)),
        update(options, state.phase),
        meter(state),
      ];
    // The button already reads "Unpacking bundle…"; a status line would only say it twice.
    case "unpacking":
      return [update(options, state.phase)];
    case "installed":
    case "restarting":
      return [
        status(t.updates.installed(state.version)),
        restart(options, state.phase),
        helper(t.updates.installedHint),
      ];
    case "failed":
      return [status(t.updates.failed[state.step], "is-failed"), helper(state.detail)];
    default:
      return [];
  }
}

const INSTALL_LABEL: Record<InstallPhase, string> = {
  available: t.updates.update,
  downloading: t.updates.installing,
  unpacking: t.updates.unpacking,
};

function update(options: UpdatesOptions, phase: InstallPhase): HTMLElement {
  const element = button("install-update", INSTALL_LABEL[phase]);
  element.disabled = phase !== "available";
  element.addEventListener("click", options.onUpdate);
  return element;
}

function restart(options: UpdatesOptions, phase: "installed" | "restarting"): HTMLElement {
  return action(
    "restart-app",
    { idle: t.updates.restart, busy: t.updates.restarting },
    phase === "restarting",
    options.onRestart,
  );
}

function action(
  focusKey: string,
  label: { idle: string; busy: string },
  busy: boolean,
  onClick: () => void,
): HTMLButtonElement {
  const element = button(focusKey, busy ? label.busy : label.idle);
  element.disabled = busy;
  element.addEventListener("click", onClick);
  return element;
}

function meter(state: Downloading): HTMLElement {
  const block = document.createElement("div");
  block.className = "download-meter";
  block.setAttribute(METER_ATTR, "");

  const track = document.createElement("div");
  track.className = "download-meter-track";
  track.setAttribute("role", "progressbar");
  track.setAttribute("aria-label", t.updates.downloadLabel);
  track.setAttribute("aria-valuemin", "0");
  track.setAttribute("aria-valuemax", "100");

  const fill = document.createElement("div");
  fill.className = "download-meter-fill";
  track.append(fill);

  const readout = document.createElement("p");
  readout.className = "download-meter-readout";

  block.append(track, readout);
  paint(block, state);
  return block;
}

function paint(block: HTMLElement, state: Downloading): void {
  const track = block.querySelector<HTMLElement>(".download-meter-track");
  const fill = block.querySelector<HTMLElement>(".download-meter-fill");
  const readout = block.querySelector<HTMLElement>(".download-meter-readout");
  if (!track || !fill || !readout) return;

  const fraction = state.total ? state.downloaded / state.total : null;
  const text = reading(state);

  fill.classList.toggle("is-indeterminate", fraction === null);
  fill.style.width = fraction === null ? "" : `${Math.min(Math.max(fraction, 0), 1) * 100}%`;
  readout.textContent = text;

  // An omitted aria-valuenow is how ARIA spells an indeterminate progressbar.
  track.setAttribute("aria-valuetext", text);
  if (fraction === null) track.removeAttribute("aria-valuenow");
  else track.setAttribute("aria-valuenow", String(Math.round(fraction * 100)));
}

function reading(state: Downloading): string {
  if (state.downloaded === 0) return t.updates.starting;

  const parts = [
    state.total === null
      ? t.updates.downloaded(formatBytes(state.downloaded))
      : t.updates.downloadedOf(formatBytes(state.downloaded), formatBytes(state.total)),
  ];
  if (state.speed !== null) parts.push(t.updates.rate(formatBytes(state.speed)));

  const left = remaining(state);
  if (left !== null) parts.push(t.updates.timeLeft(formatDuration(left)));

  return t.updates.readout(parts);
}

function remaining(state: Downloading): number | null {
  if (state.total === null || state.speed === null || state.speed <= 0) return null;
  return Math.max(state.total - state.downloaded, 0) / state.speed;
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
