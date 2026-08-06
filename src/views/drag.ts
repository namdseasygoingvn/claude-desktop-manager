import type { Group, ProfileStatus } from "../api";
import { groupOf, nextRowId, prevRowId } from "./reorder";

export interface DragOpts {
  profiles: ProfileStatus[];
  groups: Group[];
  onMove: (id: string, groupId: string | null, before: string | null) => void;
}

type Target =
  | { kind: "row"; row: HTMLElement; groupId: string | null; above: boolean; before: string | null }
  | { kind: "group"; header: HTMLElement; groupId: string | null };

let draggingId: string | null = null;

/** Wire each .row-grip as the drag source and the list as the drop surface. */
export function attachDrag(list: HTMLElement, opts: DragOpts): void {
  for (const grip of list.querySelectorAll<HTMLElement>(".row-grip")) {
    grip.addEventListener("dragstart", (event) => {
      const row = grip.closest<HTMLElement>(".profile-row");
      const id = row?.dataset.profileId ?? "";
      if (!row || !id || !event.dataTransfer) return;
      draggingId = id;
      // Without this the ghost is the 20x24 grip glyph, since the grip is the drag source.
      const rect = row.getBoundingClientRect();
      event.dataTransfer.setDragImage(row, event.clientX - rect.left, event.clientY - rect.top);
      // The ghost is rasterised after this handler returns, so fading now fades the ghost too.
      requestAnimationFrame(() => {
        if (draggingId === id) row.classList.add("is-dragging");
      });
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", id);
    });
    grip.addEventListener("dragend", clear);
  }

  list.addEventListener("dragover", (event) => {
    if (!draggingId) return;
    const target = resolveTarget(event, opts, list);
    highlight(target);
    // preventDefault is what makes a drop land, so withholding it stops the cursor from
    // promising a drop that resolves to nothing.
    if (!target) return;
    event.preventDefault();
    event.dataTransfer && (event.dataTransfer.dropEffect = "move");
  });

  list.addEventListener("drop", (event) => {
    if (!draggingId) return;
    event.preventDefault();
    const target = resolveTarget(event, opts, list);
    // Dropping a row into its own slot is a no-op; don't round-trip to the backend.
    if (target && !isOwnSlot(target)) {
      opts.onMove(draggingId, target.groupId, target.kind === "row" ? target.before : null);
    }
    clear();
  });
}

/** Target under the pointer: a row (insert before/after it), a group, or the ungrouped slot. */
function resolveTarget(event: DragEvent, opts: DragOpts, list: HTMLElement): Target | null {
  const hit = event.target as Element | null;
  if (!hit || !draggingId) return null;

  const row = hit.closest<HTMLElement>(".profile-row");
  if (row) {
    const rect = row.getBoundingClientRect();
    return rowTarget(row, opts, event.clientY < rect.top + rect.height / 2);
  }

  const header = hit.closest<HTMLElement>(".group-header");
  if (header) {
    if (header.classList.contains("is-ungrouped")) {
      return { kind: "group", header, groupId: null };
    }
    const groupId = header.dataset.groupId ?? "";
    if (groupId) return { kind: "group", header, groupId };
  }

  // The margin between sections and the space under the last row belong to no element the
  // hit test can name, so read them as "after the last row that ends above the pointer".
  const preceding = lastRowAbove(list, event.clientY);
  return preceding ? rowTarget(preceding, opts, false) : null;
}

function rowTarget(row: HTMLElement, opts: DragOpts, above: boolean): Target | null {
  const id = row.dataset.profileId ?? "";
  if (!id || id === draggingId) return null;
  return {
    kind: "row",
    row,
    groupId: groupOf(opts.groups, id),
    above,
    before: above ? id : nextRowId(row),
  };
}

function lastRowAbove(list: HTMLElement, y: number): HTMLElement | null {
  let preceding: HTMLElement | null = null;
  for (const row of list.querySelectorAll<HTMLElement>(".profile-row")) {
    if (row.getBoundingClientRect().bottom <= y) preceding = row;
  }
  return preceding;
}

/** True when the drop would land the dragged row exactly where it already sits. */
function isOwnSlot(target: Target): boolean {
  if (target.kind !== "row") return false;
  return target.above ? prevRowId(target.row) === draggingId : target.before === draggingId;
}

function highlight(target: Target | null): void {
  for (const node of document.querySelectorAll<HTMLElement>(
    ".profile-row.is-drop-before, .profile-row.is-drop-after, .group-header.is-drop-target",
  )) {
    node.classList.remove("is-drop-before", "is-drop-after", "is-drop-target");
  }
  if (!target) return;
  if (target.kind === "row") {
    target.row.classList.add(target.above ? "is-drop-before" : "is-drop-after");
  } else {
    target.header.classList.add("is-drop-target");
  }
}

function clear(): void {
  draggingId = null;
  for (const node of document.querySelectorAll<HTMLElement>(
    ".profile-row.is-dragging, .profile-row.is-drop-before, .profile-row.is-drop-after, .group-header.is-drop-target",
  )) {
    node.classList.remove("is-dragging", "is-drop-before", "is-drop-after", "is-drop-target");
  }
}
