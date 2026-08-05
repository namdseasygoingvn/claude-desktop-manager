import type { Group, ProfileStatus } from "../api";

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
      const id = grip.closest<HTMLElement>(".profile-row")?.dataset.profileId ?? "";
      if (!id || !event.dataTransfer) return;
      draggingId = id;
      grip.closest<HTMLElement>(".profile-row")?.classList.add("is-dragging");
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", id);
    });
    grip.addEventListener("dragend", clear);
  }

  list.addEventListener("dragover", (event) => {
    if (!draggingId) return;
    event.preventDefault();
    event.dataTransfer && (event.dataTransfer.dropEffect = "move");
    highlight(resolveTarget(event, opts));
  });

  list.addEventListener("drop", (event) => {
    if (!draggingId) return;
    event.preventDefault();
    const target = resolveTarget(event, opts);
    if (target) {
      const before = target.kind === "row" ? target.before : null;
      // Dropping a row into its own slot is a no-op; don't round-trip to the backend.
      if (before !== draggingId) opts.onMove(draggingId, target.groupId, before);
    }
    clear();
  });
}

/** Target under the pointer: a row (insert before/after it), a group, or the ungrouped slot. */
function resolveTarget(event: DragEvent, opts: DragOpts): Target | null {
  const hit = event.target as Element | null;
  if (!hit || !draggingId) return null;

  const row = hit.closest<HTMLElement>(".profile-row");
  if (row) {
    const id = row.dataset.profileId ?? "";
    if (!id || id === draggingId) return null;
    const rect = row.getBoundingClientRect();
    const above = event.clientY < rect.top + rect.height / 2;
    return {
      kind: "row",
      row,
      groupId: groupOf(opts.groups, id),
      above,
      before: above ? id : nextRowId(row),
    };
  }

  const header = hit.closest<HTMLElement>(".group-header");
  if (header) {
    if (header.classList.contains("is-ungrouped")) {
      return { kind: "group", header, groupId: null };
    }
    const groupId = header.dataset.groupId ?? "";
    if (groupId) return { kind: "group", header, groupId };
  }
  return null;
}

/** The row below `row` in its own section, or null when it is the last one (append). */
function nextRowId(row: HTMLElement): string | null {
  const rows = Array.from(row.parentElement?.querySelectorAll<HTMLElement>(".profile-row") ?? []);
  const next = rows[rows.indexOf(row) + 1];
  return next?.dataset.profileId ?? null;
}

function groupOf(groups: Group[], profileId: string): string | null {
  return groups.find((group) => group.profileIds.includes(profileId))?.id ?? null;
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
