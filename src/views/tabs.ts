import { Download, Settings, Shield, Users, icon, type IconNode } from "./icons";
import { t } from "./strings";

export type TabId = "profiles" | "updates" | "general" | "admin";

export const TABS: readonly TabId[] = ["profiles", "updates", "general", "admin"];

const TAB_ICONS: Record<TabId, IconNode> = {
  profiles: Users,
  updates: Download,
  general: Settings,
  admin: Shield,
};

export interface TabsOptions {
  active: TabId;
  badges: ReadonlySet<TabId>;
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
    const badged = options.badges.has(id);
    tab.append(glyph(id, badged), t.tabs[id]);
    // A coloured dot says nothing out loud; the tab has to carry the word too.
    if (badged) tab.append(note(t.tabs.attention));
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

/** The badge hangs off the icon rather than the label, so the strip never reflows when it appears. */
function glyph(id: TabId, badged: boolean): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "tab-glyph";
  wrap.append(icon(TAB_ICONS[id]));
  if (badged) {
    const dot = document.createElement("span");
    dot.className = "tab-badge";
    wrap.append(dot);
  }
  return wrap;
}

function note(text: string): HTMLElement {
  const element = document.createElement("span");
  element.className = "visually-hidden";
  element.textContent = text;
  return element;
}

function onKeydown(event: KeyboardEvent, id: TabId, onSelect: (tab: TabId) => void): void {
  const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
  if (step === 0) return;
  event.preventDefault();
  const next = TABS[(TABS.indexOf(id) + step + TABS.length) % TABS.length];
  onSelect(next);
}
