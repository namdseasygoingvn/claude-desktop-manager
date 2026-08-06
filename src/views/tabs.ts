import { t } from "./strings";

export type TabId = "profiles" | "updates" | "general";

export const TABS: readonly TabId[] = ["profiles", "updates", "general"];

export interface TabsOptions {
  active: TabId;
  onSelect: (tab: TabId) => void;
}

export function renderTabs(options: TabsOptions): HTMLElement {
  const strip = document.createElement("div");
  strip.className = "tabs";
  strip.setAttribute("role", "tablist");
  strip.setAttribute("aria-label", t.tabs.label);

  for (const id of TABS) {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = "tab";
    tab.setAttribute("role", "tab");
    tab.textContent = t.tabs[id];
    tab.dataset.focusKey = `tab-${id}`;
    const active = id === options.active;
    tab.setAttribute("aria-selected", String(active));
    // Only the selected tab is tabbable, so Tab moves into the pane rather than along the strip.
    tab.tabIndex = active ? 0 : -1;
    tab.addEventListener("click", () => options.onSelect(id));
    tab.addEventListener("keydown", (event) => onKeydown(event, id, options.onSelect));
    strip.append(tab);
  }

  return strip;
}

function onKeydown(event: KeyboardEvent, id: TabId, onSelect: (tab: TabId) => void): void {
  const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
  if (step === 0) return;
  event.preventDefault();
  const next = TABS[(TABS.indexOf(id) + step + TABS.length) % TABS.length];
  onSelect(next);
}
