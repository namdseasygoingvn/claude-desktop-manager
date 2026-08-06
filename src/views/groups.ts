import {
  createGroup,
  deleteGroup,
  renameGroup,
  setProfileGroup,
  type CdmError,
  type Group,
} from "../api";
import { openDialog, textField } from "./dialog";
import { showNotice } from "./errors";
import { ChevronDown, groupIcon, icon } from "./icons";
import { t } from "./strings";

export interface GroupHeaderProps {
  group: Group;
  members: number;
  collapsed: boolean;
  onToggle: () => void;
  onMenu: (x: number, y: number) => void;
}

/** One collapsible group header: chevron, icon, name, member count. */
export function renderGroupHeader(props: GroupHeaderProps): HTMLElement {
  const header = document.createElement("button");
  header.type = "button";
  header.className = "group-header";
  header.dataset.focusKey = `group-${props.group.id}`;
  header.dataset.groupId = props.group.id;
  header.title = props.group.name;

  const chevron = icon(ChevronDown);
  chevron.classList.add("group-chevron");
  chevron.classList.toggle("is-collapsed", props.collapsed);
  chevron.setAttribute("aria-hidden", "true");

  const name = document.createElement("span");
  name.className = "group-name";
  name.textContent = props.group.name;

  const count = document.createElement("span");
  count.className = "group-count";
  count.textContent = String(props.members);

  header.append(chevron, groupIcon(props.group.icon), name, count);
  header.addEventListener("click", () => props.onToggle());
  header.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    props.onMenu(event.clientX, event.clientY);
  });
  return header;
}

export function openNewGroupSheet(options: { onCreated: () => void }): void {
  const field = textField({
    label: t.groups.createNameLabel,
    placeholder: t.groups.createPlaceholder,
  });
  const handle = openDialog({
    title: t.groups.createTitle,
    content: field.row,
    buttons: [
      { id: "cancel", label: t.common.cancel, role: "cancel" },
      { id: "create", label: t.groups.createSubmit, role: "affirmative", onSelect: submit },
    ],
  });

  handle.setEnabled("create", false);
  field.input.addEventListener("input", () =>
    handle.setEnabled("create", field.input.value.trim().length > 0),
  );

  async function submit(): Promise<void> {
    const name = field.input.value.trim();
    if (!name) return;
    handle.setBusy(true);
    try {
      await createGroup(name);
      handle.close();
      options.onCreated();
    } catch (error) {
      handle.setBusy(false);
      const failure = error as CdmError;
      if (failure.kind !== "NameEmpty") showNotice(t.groups.createFailed(name), failure.message);
      field.input.focus();
      field.input.select();
    }
  }
}

export function openRenameGroupSheet(options: {
  group: Group;
  onRenamed: () => void;
}): void {
  const field = textField({ label: t.groups.createNameLabel, value: options.group.name });
  const handle = openDialog({
    title: t.groups.renameTitle,
    content: field.row,
    buttons: [
      { id: "cancel", label: t.common.cancel, role: "cancel" },
      { id: "rename", label: t.groups.renameSubmit, role: "affirmative", onSelect: submit },
    ],
  });

  handle.setEnabled("rename", true);
  field.input.addEventListener("input", () =>
    handle.setEnabled("rename", field.input.value.trim().length > 0),
  );

  async function submit(): Promise<void> {
    const name = field.input.value.trim();
    if (!name) return;
    handle.setBusy(true);
    try {
      await renameGroup(options.group.id, name);
      handle.close();
      options.onRenamed();
    } catch (error) {
      handle.setBusy(false);
      const failure = error as CdmError;
      if (failure.kind !== "NameEmpty") {
        showNotice(t.groups.renameFailed(options.group.name), failure.message);
      }
      field.input.focus();
      field.input.select();
    }
  }
}

export function openDeleteGroupSheet(options: {
  group: Group;
  onDeleted: () => void;
}): void {
  openDialog({
    message: t.groups.deleteMessage(options.group.name),
    informative: t.groups.deleteInformative,
    buttons: [
      {
        id: "delete",
        label: t.groups.deleteConfirm,
        role: "destructive",
        onSelect: async (handle) => {
          handle.setBusy(true);
          try {
            await deleteGroup(options.group.id);
            handle.close();
            options.onDeleted();
          } catch (error) {
            handle.setBusy(false);
            showNotice(t.groups.deleteFailed(options.group.name), (error as CdmError).message);
          }
        },
      },
      { id: "cancel", label: t.common.cancel, role: "cancel" },
    ],
  });
}

export function openAssignGroupSheet(options: {
  profileId: string;
  groups: Group[];
  currentGroupId: string | null;
  onAssigned: () => void;
}): void {
  const content = document.createElement("div");
  content.className = "assign-groups";

  const radios: HTMLInputElement[] = [];
  content.append(row(null));
  for (const group of options.groups) content.append(row(group));

  openDialog({
    title: t.groups.assignTitle,
    content,
    buttons: [
      { id: "cancel", label: t.common.cancel, role: "cancel" },
      {
        id: "assign",
        label: t.groups.assignSubmit,
        role: "affirmative",
        onSelect: async (handle) => {
          handle.setBusy(true);
          const chosen = radios.find((input) => input.checked);
          const groupId = chosen?.dataset.groupId || null;
          try {
            await setProfileGroup(options.profileId, groupId);
            handle.close();
            options.onAssigned();
          } catch (error) {
            handle.setBusy(false);
            showNotice(t.groups.assignFailed, (error as CdmError).message);
          }
        },
      },
    ],
  });

  /** The radio lives inside its label: a detached one can never be checked, so nothing is chosen. */
  function row(group: Group | null): HTMLLabelElement {
    const input = document.createElement("input");
    input.type = "radio";
    input.name = "assign-group";
    input.className = "assign-radio";
    input.dataset.groupId = group?.id ?? "";
    input.checked = (group?.id ?? null) === options.currentGroupId;
    radios.push(input);

    const name = document.createElement("span");
    name.textContent = group ? group.name : t.groups.assignNone;

    const node = document.createElement("label");
    node.className = "assign-row";
    node.append(input);
    if (group) node.append(groupIcon(group.icon));
    node.append(name);
    return node;
  }
}
