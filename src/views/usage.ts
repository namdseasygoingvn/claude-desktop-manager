import type { Usage } from "../api";
import { t } from "./strings";

const WARN = 75;
const CRITICAL = 90;
/** Further out than this, a weekly reset reads better as a weekday than as a running countdown. */
const COUNTDOWN_WINDOW = 24 * 60 * 60 * 1000;

type Variant = "row" | "detail";

type ResetFormat = (resetsAt: number, now: number, variant: Variant) => string;

interface Limit {
  name: string;
  short: string;
  percent: number;
  resetsAt: number | null;
  resetFormat: ResetFormat;
}

/** Null when there is nothing worth showing — the caller then renders no usage at all. */
export function usageBars(usage: Usage, variant: Variant): HTMLElement | null {
  const limits = reported(usage);
  if (limits.length === 0) return null;
  const now = Date.now();

  const block = document.createElement("div");
  block.className = `usage-bars is-${variant}`;
  // Sidebar meters are decoration: the row carries the same figures in its own aria-label,
  // and a progressbar nested in a role="option" is not reliably announced anyway.
  if (variant === "row") block.setAttribute("aria-hidden", "true");
  for (const limit of limits) block.append(meter(limit, variant, now));

  // A sidebar row prints how old its sample is already, and has no width to spare for a sentence.
  const note = variant === "detail" ? noteText(usage, limits, now) : null;
  if (note) block.append(noteLine(note));
  return block;
}

export function usageSummary(usage: Usage): string | null {
  const limits = reported(usage);
  if (limits.length === 0) return null;
  return limits.map((limit) => `${limit.name} ${reading(limit.percent)}`).join(", ");
}

function reported(usage: Usage): Limit[] {
  return [
    {
      name: t.usage.fiveHour,
      short: t.usage.fiveHourShort,
      percent: usage.fiveHour,
      resetsAt: usage.fiveHourResetsAt,
      resetFormat: countdown,
    },
    {
      name: t.usage.weekly,
      short: t.usage.weeklyShort,
      percent: usage.sevenDay,
      resetsAt: usage.sevenDayResetsAt,
      resetFormat: dayUnlessSoon,
    },
  ].filter((limit): limit is Limit => limit.percent !== null);
}

function countdown(resetsAt: number, now: number, variant: Variant): string {
  const left = resetsAt - now;
  return variant === "row" ? t.usage.resetsInShort(left) : t.usage.resetsIn(left);
}

/** Claude Desktop's own popup counts the 5-hour limit down but dates the weekly one. */
function dayUnlessSoon(resetsAt: number, now: number, variant: Variant): string {
  if (resetsAt - now < COUNTDOWN_WINDOW) return countdown(resetsAt, now, variant);
  return variant === "row" ? t.usage.resetsAtDayShort(resetsAt) : t.usage.resetsAtDay(resetsAt);
}

function expired(resetsAt: number | null, now: number): boolean {
  return resetsAt !== null && resetsAt <= now;
}

function resetText(limit: Limit, variant: Variant, now: number): string | null {
  const { resetsAt } = limit;
  if (resetsAt === null || expired(resetsAt, now)) return null;
  return limit.resetFormat(resetsAt, now, variant);
}

/** A reset in the past means Claude Desktop stopped sampling, so the percentages are old too. */
function noteText(usage: Usage, limits: Limit[], now: number): string | null {
  const stale = limits.some((limit) => expired(limit.resetsAt, now));
  return stale ? t.usage.staleReading(usage.sampledAt) : missingReason(usage.source);
}

function missingReason(source: Usage["source"]): string | null {
  switch (source) {
    case "noCacheEntry":
      return t.usage.noCacheEntry;
    case "cacheUnreadable":
      return t.usage.cacheUnreadable;
    default:
      return null;
  }
}

function noteLine(text: string): HTMLElement {
  const line = document.createElement("p");
  line.className = "helper usage-note";
  line.textContent = text;
  return line;
}

function reading(percent: number): string {
  return `${percent}%`;
}

function valueText(percent: number, reset: string | null): string {
  return reset ? `${reading(percent)}, ${reset}` : reading(percent);
}

function meter(limit: Limit, variant: Variant, now: number): HTMLElement {
  const reset = resetText(limit, variant, now);

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
    track.setAttribute("aria-valuetext", valueText(limit.percent, reset));
  }

  const fill = document.createElement("div");
  fill.className = "usage-meter-fill";
  fill.style.width = reading(Math.min(Math.max(limit.percent, 0), 100));

  track.append(fill);
  row.append(label, value, track);

  if (reset) {
    const resets = document.createElement("span");
    resets.className = "usage-meter-reset";
    resets.textContent = reset;
    row.append(resets);
  }
  return row;
}
