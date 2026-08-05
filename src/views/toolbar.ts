import { openMenu, type MenuEntry } from "./menu";
import { isMac } from "./platform";
import { nouns, t } from "./strings";

export interface CommandActions {
  newProfile: () => void;
  launch: (id: string) => void;
  rename: () => void;
  editConfig: () => void;
  reveal: () => void;
  remove: () => void;
  adopt: () => void;
  copyDiagnostics: () => void;
}

export interface CommandState {
  hasSelection: boolean;
  hasCandidates: boolean;
}

export function renderToolbar(state: CommandState, actions: CommandActions): HTMLElement {
  const bar = document.createElement("div");
  bar.className = "toolbar";

  const create = iconButton(t.list.newProfile, isMac ? "⊞" : `⊞ ${t.list.newProfile}`);
  create.dataset.focusKey = "new-profile";
  create.addEventListener("click", actions.newProfile);
  bar.append(create);

  if (isMac) {
    const remove = iconButton(t.list.deleteProfile, "⊟");
    remove.disabled = !state.hasSelection;
    remove.addEventListener("click", actions.remove);
    bar.append(remove);
  }

  const more = iconButton(t.list.moreActions, isMac ? "⋯" : "▾");
  more.setAttribute("aria-haspopup", "menu");
  more.addEventListener("click", () => openMenu(moreMenu(state, actions), more));
  bar.append(more);

  return bar;
}

/** The permanent entry point to adoption; the discovery banner is only a shortcut to it. */
function moreMenu(state: CommandState, actions: CommandActions): MenuEntry[] {
  const addExisting: MenuEntry = {
    label: t.list.addExisting,
    disabled: !state.hasCandidates,
    onSelect: actions.adopt,
  };
  if (!isMac) return [addExisting];
  return [
    addExisting,
    { label: t.detail.rename, disabled: !state.hasSelection, onSelect: actions.rename },
    { label: nouns.revealItem, disabled: !state.hasSelection, onSelect: actions.reveal },
    { label: t.list.copyDiagnostics, onSelect: actions.copyDiagnostics },
  ];
}

export function rowMenu(id: string, actions: CommandActions): MenuEntry[] {
  return [
    { label: t.detail.launch, onSelect: () => actions.launch(id) },
    { label: t.detail.rename, onSelect: actions.rename },
    { label: t.detail.editConfig, onSelect: actions.editConfig },
    { label: nouns.revealItem, onSelect: actions.reveal },
    "separator",
    { label: t.detail.delete, destructive: true, onSelect: actions.remove },
  ];
}

function iconButton(label: string, glyph: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "toolbar-button";
  button.title = label;
  button.setAttribute("aria-label", label);
  button.textContent = glyph;
  return button;
}
