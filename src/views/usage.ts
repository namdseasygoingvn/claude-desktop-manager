import type { Usage } from "../api";
import { t } from "./strings";

const WARN = 75;
const CRITICAL = 90;

type Variant = "row" | "detail";

interface Limit {
  name: string;
  short: string;
  percent: number;
}

/** Null when there is nothing worth showing — the caller then renders no usage at all. */
export function usageBars(usage: Usage, variant: Variant): HTMLElement | null {
  const limits = reported(usage);
  if (limits.length === 0) return null;

  const block = document.createElement("div");
  block.className = `usage-bars is-${variant}`;
  // Sidebar meters are decoration: the row carries the same figures in its own aria-label,
  // and a progressbar nested in a role="option" is not reliably announced anyway.
  if (variant === "row") block.setAttribute("aria-hidden", "true");
  for (const limit of limits) block.append(meter(limit, variant));
  return block;
}

export function usageSummary(usage: Usage): string | null {
  const limits = reported(usage);
  if (limits.length === 0) return null;
  return limits.map((limit) => `${limit.name} ${reading(limit.percent)}`).join(", ");
}

function reported(usage: Usage): Limit[] {
  return [
    { name: t.usage.fiveHour, short: t.usage.fiveHourShort, percent: usage.fiveHour },
    { name: t.usage.weekly, short: t.usage.weeklyShort, percent: usage.sevenDay },
  ].filter((limit): limit is Limit => limit.percent !== null);
}

function reading(percent: number): string {
  return `${percent}%`;
}

function meter(limit: Limit, variant: Variant): HTMLElement {
  const row = document.createElement("div");
  row.className = "usage-meter";
  if (limit.percent >= CRITICAL) row.classList.add("is-critical");
  else if (limit.percent >= WARN) row.classList.add("is-warn");

  const label = document.createElement("span");
  label.className = "usage-meter-label";
  label.textContent = variant === "row" ? limit.short : limit.name;

  const value = document.createElement("span");
  value.className = "usage-meter-value";
  value.textContent = reading(limit.percent);

  const track = document.createElement("div");
  track.className = "usage-meter-track";
  if (variant === "detail") {
    track.setAttribute("role", "progressbar");
    track.setAttribute("aria-label", limit.name);
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", "100");
    track.setAttribute("aria-valuenow", String(limit.percent));
    track.setAttribute("aria-valuetext", reading(limit.percent));
  }

  const fill = document.createElement("div");
  fill.className = "usage-meter-fill";
  fill.style.width = reading(Math.min(Math.max(limit.percent, 0), 100));

  track.append(fill);
  row.append(label, value, track);
  return row;
}
