import { t } from "./strings";

export interface GeneralOptions {
  openPreferencesAtStart: boolean;
  launchAtLogin: boolean;
  error: string | null;
  onOpenPreferencesAtStart: (enabled: boolean) => void;
  onLaunchAtLogin: (enabled: boolean) => void;
}

export function renderGeneral(options: GeneralOptions): HTMLElement {
  const pane = document.createElement("div");
  pane.className = "settings-pane";

  const heading = document.createElement("h1");
  heading.textContent = t.general.heading;

  pane.append(
    heading,
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

function failure(detail: string): HTMLElement {
  const line = document.createElement("p");
  line.className = "settings-status is-failed";
  line.setAttribute("role", "status");
  line.textContent = detail;
  return line;
}
