import { ChevronDown, Ellipsis, icon, Minus, Plus, RefreshCw, type IconNode } from "./icons";
import { openMenu, type MenuEntry } from "./menu";
import { isMac } from "./platform";
import { nouns, t } from "./strings";

export interface CommandActions {
  newProfile: () => void;
  newGroup: () => void;
  launch: (id: string) => void;
  rename: () => void;
  assignToGroup: () => void;
  editConfig: () => void;
  reveal: () => void;
  remove: () => void;
  adopt: () => void;
  copyDiagnostics: () => void;
  refresh: () => void;
  /** null = membership unknown (status not loaded yet); the caller must hide the entry, not disable it. */
  isSessionSyncMember: (id: string) => boolean | null;
  toggleSessionSync: (id: string) => void;
}

export interface CommandState {
  hasSelection: boolean;
  hasCandidates: boolean;
  selectedIsDefaultInstall: boolean;
}

export function renderToolbar(state: CommandState, actions: CommandActions): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "toolbar";

  const create = iconButton(t.list.newProfile, Plus, !isMac);
  create.dataset.focusKey = "new-profile";
  create.addEventListener("click", actions.newProfile);
  bar.append(create);

  if (isMac) {
    const remove = iconButton(t.list.deleteProfile, Minus);
    remove.disabled = !state.hasSelection || state.selectedIsDefaultInstall;
    remove.addEventListener("click", actions.remove);
    bar.append(remove);
  }

  const more = iconButton(t.list.moreActions, isMac ? Ellipsis : ChevronDown);
  more.setAttribute("aria-haspopup", "menu");
  more.addEventListener("click", () => openMenu(moreMenu(state, actions), more));
  bar.append(more);

  const reload = iconButton(t.usage.refresh, RefreshCw);
  reload.dataset.focusKey = "refresh";
  reload.addEventListener("click", actions.refresh);
  bar.append(reload);

  return bar;
}

/** The permanent entry point to adoption; the discovery banner is only a shortcut to it. */
function moreMenu(state: CommandState, actions: CommandActions): MenuEntry[] {
  const addExisting: MenuEntry = {
    label: t.list.addExisting,
    disabled: !state.hasCandidates,
    onSelect: actions.adopt,
  };
  const newGroup: MenuEntry = { label: t.groups.newGroup, onSelect: actions.newGroup };
  if (!isMac) return [newGroup, addExisting];
  return [
    newGroup,
    addExisting,
    { label: t.detail.rename, disabled: !state.hasSelection, onSelect: actions.rename },
    { label: nouns.revealItem, disabled: !state.hasSelection, onSelect: actions.reveal },
    { label: t.list.copyDiagnostics, onSelect: actions.copyDiagnostics },
  ];
}

export function rowMenu(id: string, actions: CommandActions, isDefaultInstall: boolean): MenuEntry[] {
  const member = actions.isSessionSyncMember(id);
  const sync: MenuEntry[] =
    member === null
      ? []
      : [
          {
            label: member ? t.sessionSync.memberLabel : t.sessionSync.label,
            onSelect: () => actions.toggleSessionSync(id),
          },
        ];
  return [
    { label: t.detail.launch, onSelect: () => actions.launch(id) },
    { label: t.detail.rename, onSelect: actions.rename },
    { label: t.detail.editConfig, onSelect: actions.editConfig },
    { label: t.groups.assignToGroup, onSelect: actions.assignToGroup },
    { label: nouns.revealItem, onSelect: actions.reveal },
    ...sync,
    "separator",
    { label: t.detail.delete, destructive: true, disabled: isDefaultInstall, onSelect: actions.remove },
  ];
}

function iconButton(label: string, glyph: IconNode, withLabel = false): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "toolbar-button";
  button.title = label;
  button.setAttribute("aria-label", label);
  button.append(icon(glyph));
  if (withLabel) {
    const text = document.createElement("span");
    text.textContent = label;
    button.append(text);
  }
  return button;
}
