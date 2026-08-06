import type { Theme } from "../api";
import { t } from "./strings";

const THEMES: readonly Theme[] = ["light", "dark", "system"];

export interface GeneralOptions {
  openPreferencesAtStart: boolean;
  launchAtLogin: boolean;
  showUsageLimits: boolean;
  theme: Theme;
  error: string | null;
  onOpenPreferencesAtStart: (enabled: boolean) => void;
  onLaunchAtLogin: (enabled: boolean) => void;
  onShowUsageLimits: (enabled: boolean) => void;
  onTheme: (theme: Theme) => void;
}

export function renderGeneral(options: GeneralOptions): HTMLElement {
  const pane = document.createElement("div");
  pane.className = "settings-pane";

  const heading = document.createElement("h1");
  heading.textContent = t.general.heading;

  pane.append(
    heading,
    segmented({
      focusKey: "theme",
      label: t.general.theme,
      hint: t.general.themeHint,
      value: options.theme,
      values: THEMES,
      name: (theme) => t.general.themes[theme],
      onChange: options.onTheme,
    }),
    toggle({
      focusKey: "open-at-start",
      label: t.general.openAtStart,
      hint: t.general.openAtStartHint,
      checked: options.openPreferencesAtStart,
      onChange: options.onOpenPreferencesAtStart,
    }),
    toggle({
      focusKey: "launch-at-login",
      label: t.general.launchAtLogin,
      hint: t.general.launchAtLoginHint,
      checked: options.launchAtLogin,
      onChange: options.onLaunchAtLogin,
    }),
    toggle({
      focusKey: "show-usage-limits",
      label: t.usage.show,
      hint: t.usage.showHint,
      checked: options.showUsageLimits,
      onChange: options.onShowUsageLimits,
    }),
  );

  if (options.error) pane.append(failure(options.error));

  return pane;
}

interface ToggleOptions {
  focusKey: string;
  label: string;
  hint: string;
  checked: boolean;
  onChange: (enabled: boolean) => void;
}

function toggle(options: ToggleOptions): HTMLElement {
  const row = document.createElement("label");
  row.className = "settings-toggle";

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = options.checked;
  input.dataset.focusKey = options.focusKey;
  input.addEventListener("change", () => options.onChange(input.checked));

  const label = document.createElement("span");
  label.textContent = options.label;

  const hint = document.createElement("span");
  hint.className = "helper";
  hint.textContent = options.hint;

  const text = document.createElement("span");
  text.className = "settings-toggle-text";
  text.append(label, hint);

  row.append(input, text);
  return row;
}

interface SegmentedOptions<T extends string> {
  focusKey: string;
  label: string;
  hint: string;
  value: T;
  values: readonly T[];
  name: (value: T) => string;
  onChange: (value: T) => void;
}

function segmented<T extends string>(options: SegmentedOptions<T>): HTMLElement {
  const row = document.createElement("div");
  row.className = "settings-segmented";

  const label = document.createElement("span");
  label.className = "settings-label";
  label.id = `${options.focusKey}-label`;
  label.textContent = options.label;

  const bar = document.createElement("div");
  bar.className = "segmented";
  bar.setAttribute("role", "radiogroup");
  bar.setAttribute("aria-labelledby", label.id);

  for (const value of options.values) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "segment";
    button.setAttribute("role", "radio");
    button.textContent = options.name(value);
    button.dataset.focusKey = `${options.focusKey}-${value}`;
    const checked = value === options.value;
    button.setAttribute("aria-checked", String(checked));
    // Only the checked segment is tabbable, so Tab leaves the group rather than walking it.
    button.tabIndex = checked ? 0 : -1;
    button.addEventListener("click", () => options.onChange(value));
    button.addEventListener("keydown", (event) => onKeydown(event, value, options));
    bar.append(button);
  }

  const hint = document.createElement("span");
  hint.className = "helper";
  hint.textContent = options.hint;

  row.append(label, bar, hint);
  return row;
}

/** Arrows move the choice itself, which is how a radiogroup is expected to behave. */
function onKeydown<T extends string>(
  event: KeyboardEvent,
  value: T,
  options: SegmentedOptions<T>,
): void {
  const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
  const back = event.key === "ArrowLeft" || event.key === "ArrowUp";
  if (!forward && !back) return;
  event.preventDefault();
  const step = forward ? 1 : -1;
  const { values } = options;
  options.onChange(values[(values.indexOf(value) + step + values.length) % values.length]);
}

function failure(detail: string): HTMLElement {
  const line = document.createElement("p");
  line.className = "settings-status is-failed";
  line.setAttribute("role", "status");
  line.textContent = detail;
  return line;
}
