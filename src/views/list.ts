import type { ProfileStatus } from "../api";
import { lastUsedShort, t } from "./strings";

export const FILTER_THRESHOLD = 10;

export interface ListProps {
  profiles: ProfileStatus[];
  selectedId: string | null;
  missingIds: Set<string>;
  filter: string;
  onSelect: (id: string) => void;
  onActivate: (id: string) => void;
  onFilter: (value: string) => void;
  onContextMenu: (id: string, x: number, y: number) => void;
}

const collator = new Intl.Collator(undefined, { sensitivity: "base" });

/** Alphabetical so the click target never moves between clicks; createdAt breaks ties. */
export function sortProfiles(profiles: ProfileStatus[]): ProfileStatus[] {
  return [...profiles].sort((a, b) => {
    const byName = collator.compare(a.profile.name, b.profile.name);
    return byName !== 0 ? byName : a.profile.createdAt.localeCompare(b.profile.createdAt);
  });
}

export function filterProfiles(profiles: ProfileStatus[], filter: string): ProfileStatus[] {
  const needle = filter.trim().toLowerCase();
  if (!needle) return profiles;
  return profiles.filter((status) => status.profile.name.toLowerCase().includes(needle));
}

export function renderSidebar(props: ListProps): HTMLElement {
  const sidebar = document.createElement("aside");
  sidebar.className = "sidebar";

  const header = document.createElement("h2");
  header.className = "sidebar-header";
  header.id = "profiles-header";
  header.textContent = t.list.header;
  sidebar.append(header);

  if (props.profiles.length > FILTER_THRESHOLD) {
    const filter = document.createElement("input");
    filter.type = "search";
    filter.className = "filter";
    filter.value = props.filter;
    filter.placeholder = t.list.filterPlaceholder;
    filter.setAttribute("aria-label", t.list.filterLabel);
    filter.dataset.focusKey = "filter";
    filter.addEventListener("input", () => props.onFilter(filter.value));
    sidebar.append(filter);
  }

  sidebar.append(renderList(props));
  return sidebar;
}

function renderList(props: ListProps): HTMLElement {
  const list = document.createElement("div");
  list.className = "profile-list";
  list.setAttribute("role", "listbox");
  list.setAttribute("aria-labelledby", "profiles-header");

  const visible = filterProfiles(sortProfiles(props.profiles), props.filter);

  for (const status of visible) {
    list.append(renderRow(status, props));
  }

  list.addEventListener("keydown", (event) => {
    const index = visible.findIndex((status) => status.profile.id === props.selectedId);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = visible[Math.min(Math.max(index + step, 0), visible.length - 1)];
      if (next) props.onSelect(next.profile.id);
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const next = event.key === "Home" ? visible[0] : visible[visible.length - 1];
      if (next) props.onSelect(next.profile.id);
    } else if (event.key === "Enter" && props.selectedId) {
      event.preventDefault();
      props.onActivate(props.selectedId);
    }
  });

  return list;
}

function renderRow(status: ProfileStatus, props: ListProps): HTMLElement {
  const { profile } = status;
  const selected = profile.id === props.selectedId;
  const missing = props.missingIds.has(profile.id);
  const running = status.runningPid !== null;

  const row = document.createElement("div");
  row.className = "profile-row";
  row.setAttribute("role", "option");
  row.setAttribute("aria-selected", String(selected));
  row.setAttribute("aria-label", t.list.rowLabel(profile.name, running));
  row.tabIndex = selected ? 0 : -1;
  row.dataset.focusKey = `row-${profile.id}`;
  row.classList.toggle("is-selected", selected);
  row.classList.toggle("is-missing", missing);

  const bullet = document.createElement("span");
  bullet.className = "bullet";
  bullet.setAttribute("aria-hidden", "true");
  bullet.textContent = running ? "●" : "";

  const name = document.createElement("span");
  name.className = "row-name";
  name.textContent = profile.name;

  const secondary = document.createElement("span");
  secondary.className = "row-secondary";
  secondary.textContent = missing
    ? t.list.missing
    : running
      ? t.list.running
      : profile.lastUsedAt
        ? lastUsedShort(profile.lastUsedAt)
        : t.list.neverLaunched;

  row.append(bullet, name, secondary);
  row.addEventListener("click", () => props.onSelect(profile.id));
  row.addEventListener("dblclick", () => props.onActivate(profile.id));
  row.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    props.onSelect(profile.id);
    props.onContextMenu(profile.id, event.clientX, event.clientY);
  });
  return row;
}
