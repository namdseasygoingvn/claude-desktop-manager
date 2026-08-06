import type { Usage } from "../api";
import { t } from "./strings";

/** Null when there is nothing worth showing — the row then renders no usage at all. */
export function usageText(usage: Usage | null): string | null {
  if (!usage) return null;
  const reported = [usage.fiveHour, usage.sevenDay].filter(
    (value): value is number => value !== null,
  );
  if (reported.length === 0) return null;
  return `${reported.map(percent).join(" / ")} · ${t.usage.age(usage.sampledAt)}`;
}

export function usageBars(usage: Usage): HTMLElement {
  const block = document.createElement("div");
  block.className = "usage-bars";
  if (usage.fiveHour !== null) block.append(bar(t.usage.fiveHour, usage.fiveHour));
  if (usage.sevenDay !== null) block.append(bar(t.usage.weekly, usage.sevenDay));
  return block;
}

function percent(value: number): string {
  return `${value}%`;
}

function bar(label: string, value: number): HTMLElement {
  const row = document.createElement("div");
  row.className = "usage-bar";

  const name = document.createElement("span");
  name.className = "usage-bar-label";
  name.textContent = label;

  const reading = document.createElement("span");
  reading.className = "usage-bar-value";
  reading.textContent = percent(value);

  const track = document.createElement("div");
  track.className = "usage-bar-track";
  track.setAttribute("role", "progressbar");
  track.setAttribute("aria-label", label);
  track.setAttribute("aria-valuemin", "0");
  track.setAttribute("aria-valuemax", "100");
  track.setAttribute("aria-valuenow", String(value));
  track.setAttribute("aria-valuetext", percent(value));

  const fill = document.createElement("div");
  fill.className = "usage-bar-fill";
  fill.style.width = percent(Math.min(Math.max(value, 0), 100));

  track.append(fill);
  row.append(name, reading, track);
  return row;
}
