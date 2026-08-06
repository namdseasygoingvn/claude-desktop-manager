import type { Group } from "../api";
import { matches, shortcuts } from "./platform";

/** Where a profile ends up: which group owns it, and which row it sits in front of. */
export interface Move {
  id: string;
  groupId: string | null;
  before: string | null;
}

export function groupOf(groups: Group[], profileId: string): string | null {
  return groups.find((group) => group.profileIds.includes(profileId))?.id ?? null;
}

/** The row below `row` in its own section, or null when it is the last one (append). */
export function nextRowId(row: HTMLElement): string | null {
  return sibling(row, 1)?.dataset.profileId ?? null;
}

/** The row above `row` in its own section, or null when it is the first one. */
export function prevRowId(row: HTMLElement): string | null {
  return sibling(row, -1)?.dataset.profileId ?? null;
}

function sibling(row: HTMLElement, step: number): HTMLElement | null {
  const rows = Array.from(row.parentElement?.querySelectorAll<HTMLElement>(".profile-row") ?? []);
  const index = rows.indexOf(row) + step;
  return index < 0 ? null : (rows[index] ?? null);
}

/** -1 to nudge a row up, 1 down, 0 when the event is not a reorder shortcut. */
export function reorderStep(event: KeyboardEvent): number {
  if (matches(event, shortcuts.moveUp)) return -1;
  if (matches(event, shortcuts.moveDown)) return 1;
  return 0;
}

/**
 * Where `row` lands when nudged one slot: swapped with its neighbour inside the section, or —
 * once it runs out of neighbours — crossing into the next section at the edge it came from.
 */
export function neighbourMove(
  list: HTMLElement,
  row: HTMLElement,
  groups: Group[],
  step: number,
): Move | null {
  const id = row.dataset.profileId ?? "";
  if (!id) return null;
  const groupId = groupOf(groups, id);

  const neighbour = sibling(row, step);
  if (neighbour) {
    const neighbourId = neighbour.dataset.profileId ?? null;
    return { id, groupId, before: step > 0 ? nextRowId(neighbour) : neighbourId };
  }
  return crossSection(list, id, groupId, step);
}

/**
 * Sections are found by their header rather than by their rows, so a section the nudge just
 * emptied is still reachable — otherwise the move that emptied it could not be undone.
 */
function crossSection(
  list: HTMLElement,
  id: string,
  groupId: string | null,
  step: number,
): Move | null {
  // A collapsed section renders no row container, and a profile nudged into one would vanish.
  const headers = Array.from(list.querySelectorAll<HTMLElement>(".group-header")).filter(
    (header) => header.nextElementSibling?.classList.contains("group-rows"),
  );
  const index = headers.findIndex((header) => (header.dataset.groupId ?? null) === groupId);
  const target = index < 0 ? undefined : headers[index + step];
  if (!target) return null;
  return {
    id,
    groupId: target.dataset.groupId ?? null,
    before: step > 0 ? firstRowId(target) : null,
  };
}

function firstRowId(header: HTMLElement): string | null {
  const row = header.nextElementSibling?.querySelector<HTMLElement>(".profile-row");
  return row?.dataset.profileId ?? null;
}
