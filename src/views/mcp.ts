import type { McpStatus } from "../api";
import { t } from "./strings";

/** Matches `mcp::LOWEST_PORT`; the backend refuses anything under it either way. */
const LOWEST_PORT = 1024;
const HIGHEST_PORT = 65535;

const FACTS_ATTR = "data-mcp-facts";
const LOG_ATTR = "data-mcp-log";
const CLEAR_ATTR = "data-mcp-clear";

/** Close enough to the newest line to count as following it rather than reading further up. */
const STICK_PX = 24;

export interface McpOptions {
  status: McpStatus;
  logs: string[];
  /** What the user has typed but not committed. Null while the field shows the stored port. */
  portDraft: string | null;
  error: string | null;
  onEnabled: (enabled: boolean) => void;
  onPortDraft: (value: string | null) => void;
  onPortCommit: (value: string) => void;
  onCopyUrl: () => void;
  onClearLogs: () => void;
}

export function renderMcp(options: McpOptions): HTMLElement {
  const section = document.createElement("section");
  section.className = "settings-section";

  const heading = document.createElement("h2");
  heading.textContent = t.mcp.heading;

  // The environment does not care what the stored switches say, so neither should the controls.
  const frozen = options.status.envOverride !== null;

  section.append(heading, enable(options, frozen), port(options, frozen), ...outcome(options));
  if (options.error) section.append(line(options.error, "is-failed"));
  section.append(log(options));

  return section;
}

/**
 * Requests, uptime, and the log all move on their own. Re-rendering the pane every second to
 * show that would take the log's scroll position and the port field's caret with it, so the
 * poll writes only the text that changed. Everything else moves when the user moves it.
 */
export function paintMcp(root: ParentNode, status: McpStatus, logs: string[]): void {
  const measured = root.querySelector<HTMLElement>(`[${FACTS_ATTR}]`);
  if (measured) measured.textContent = factsText(status);

  const clear = root.querySelector<HTMLButtonElement>(`[${CLEAR_ATTR}]`);
  if (clear) clear.disabled = logs.length === 0;

  const lines = root.querySelector<HTMLElement>(`[${LOG_ATTR}]`);
  if (lines) paintLog(lines, logs);
}

function enable(options: McpOptions, frozen: boolean): HTMLElement {
  const row = document.createElement("label");
  row.className = "settings-toggle";

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = options.status.enabled;
  input.disabled = frozen;
  input.dataset.focusKey = "mcp-enabled";
  input.addEventListener("change", () => options.onEnabled(input.checked));

  const label = document.createElement("span");
  label.textContent = t.mcp.enable;

  const text = document.createElement("span");
  text.className = "settings-toggle-text";
  text.append(label, helper(t.mcp.enableHint));

  row.append(input, text);
  return row;
}

function port(options: McpOptions, frozen: boolean): HTMLElement {
  const block = document.createElement("div");
  block.className = "settings-field";

  const input = document.createElement("input");
  input.type = "number";
  input.className = "settings-number";
  input.id = "mcp-port";
  input.min = String(LOWEST_PORT);
  input.max = String(HIGHEST_PORT);
  input.disabled = frozen;
  input.value = options.portDraft ?? String(options.status.port);
  input.dataset.focusKey = "mcp-port";

  const label = document.createElement("label");
  label.className = "settings-label";
  label.htmlFor = input.id;
  label.textContent = t.mcp.port;

  input.addEventListener("input", () => options.onPortDraft(input.value));
  // Fires on Enter and on blur, so committing needs no button of its own.
  input.addEventListener("change", () => options.onPortCommit(input.value));
  input.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    options.onPortDraft(null);
  });

  const row = document.createElement("div");
  row.className = "settings-field-row";
  row.append(label, input);

  block.append(row, helper(t.mcp.portHint));
  return block;
}

/** Whether it is on, where to reach it, and — the case needing the most words — why not. */
function outcome(options: McpOptions): HTMLElement[] {
  const { status } = options;
  const parts: HTMLElement[] = [];

  if (status.listening && status.url) {
    parts.push(connection(status.url, options.onCopyUrl), facts(status));
  } else if (status.error) {
    parts.push(line(t.mcp.failed(status.port, status.error), "is-failed"));
  } else {
    parts.push(line(t.mcp.off));
  }

  if (status.envOverride !== null) parts.push(helper(t.mcp.overridden(status.envOverride)));
  return parts;
}

function connection(url: string, onCopy: () => void): HTMLElement {
  const row = document.createElement("div");
  row.className = "settings-field-row";

  const reachable = line(t.mcp.listening(url), "is-ok");
  reachable.classList.add("mcp-url");

  const copy = document.createElement("button");
  copy.type = "button";
  copy.className = "button";
  copy.dataset.focusKey = "mcp-copy-url";
  copy.textContent = t.mcp.copy;
  copy.addEventListener("click", onCopy);

  row.append(reachable, copy);
  return row;
}

function facts(status: McpStatus): HTMLElement {
  const element = document.createElement("p");
  element.className = "settings-value";
  element.setAttribute(FACTS_ATTR, "");
  element.textContent = factsText(status);
  return element;
}

function factsText(status: McpStatus): string {
  const parts = [
    t.mcp.server(status.name, status.version),
    t.mcp.protocol(status.protocolVersion),
    t.mcp.tools(status.tools),
    t.mcp.requests(status.requests),
  ];
  if (status.uptimeSeconds !== null) parts.push(t.mcp.uptime(status.uptimeSeconds));
  return t.mcp.facts(parts);
}

function log(options: McpOptions): HTMLElement {
  const block = document.createElement("div");
  block.className = "mcp-log";

  const label = document.createElement("span");
  label.className = "settings-label";
  label.textContent = t.mcp.log;

  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "button";
  clear.dataset.focusKey = "mcp-clear-log";
  clear.setAttribute(CLEAR_ATTR, "");
  clear.textContent = t.mcp.clearLog;
  clear.disabled = options.logs.length === 0;
  clear.addEventListener("click", options.onClearLogs);

  const header = document.createElement("div");
  header.className = "settings-field-row";
  header.append(label, clear);

  const lines = document.createElement("pre");
  lines.className = "mcp-log-lines";
  lines.setAttribute(LOG_ATTR, "");
  lines.setAttribute("role", "log");
  lines.setAttribute("aria-label", t.mcp.log);
  lines.tabIndex = 0;
  write(lines, options.logs);
  // Nothing has been laid out yet, so the scroll height only exists from the next frame on.
  requestAnimationFrame(() => {
    lines.scrollTop = lines.scrollHeight;
  });

  block.append(header, lines, helper(t.mcp.logHint));
  return block;
}

/** Stay pinned to the newest line, unless the user has scrolled up to read something. */
function paintLog(element: HTMLElement, logs: string[]): void {
  const following =
    element.scrollHeight - element.scrollTop - element.clientHeight < STICK_PX;
  write(element, logs);
  if (following) element.scrollTop = element.scrollHeight;
}

function write(element: HTMLElement, logs: string[]): void {
  element.textContent = logs.length > 0 ? logs.join("\n") : t.mcp.logEmpty;
  element.classList.toggle("is-empty", logs.length === 0);
}

function line(text: string, tone?: "is-ok" | "is-failed"): HTMLElement {
  const element = document.createElement("p");
  element.className = tone ? `settings-status ${tone}` : "settings-status";
  element.setAttribute("role", "status");
  element.textContent = text;
  return element;
}

function helper(text: string): HTMLElement {
  const element = document.createElement("span");
  element.className = "helper";
  element.textContent = text;
  return element;
}
