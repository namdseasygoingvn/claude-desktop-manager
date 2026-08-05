import {
  appVersion,
  checkForUpdates,
  hideWindow,
  installUpdate,
  launchProfile,
  listAdoptable,
  listProfiles,
  locateFolder,
  onTrayEvent,
  onWindowShown,
  openConfig,
  revealProfile,
  type AdoptCandidate,
  type CdmError,
  type Profile,
  type ProfileStatus,
} from "./api";
import { openAdoptSheet, renderAdoptBanner } from "./views/adopt";
import { openCreateSheet } from "./views/create";
import { confirmDelete, confirmRemoveFromList } from "./views/delete";
import { renderDetail } from "./views/detail";
import { isDialogOpen } from "./views/dialog";
import { renderEmpty } from "./views/empty";
import {
  copyDiagnostics,
  renderRegistryError,
  reportError,
  showBinaryNotFound,
  showLaunchFailed,
} from "./views/errors";
import { filterProfiles, renderSidebar, sortProfiles } from "./views/list";
import { openMenu } from "./views/menu";
import { matches, platform, shortcuts } from "./views/platform";
import { openRenameSheet } from "./views/rename";
import { renderTabs, type TabId } from "./views/tabs";
import { renderToolbar, rowMenu, type CommandActions } from "./views/toolbar";
import { renderUpdates, type UpdateState } from "./views/updates";

const LAUNCH_FEEDBACK_MS = 3000;

const root = document.getElementById("app") as HTMLElement;

const state = {
  tab: "profiles" as TabId,
  profiles: [] as ProfileStatus[],
  candidates: [] as AdoptCandidate[],
  selectedId: null as string | null,
  filter: "",
  launching: new Set<string>(),
  missing: new Set<string>(),
  bannerDismissed: false,
  fatal: null as CdmError | null,
  version: "",
  update: { phase: "idle" } as UpdateState,
};

document.documentElement.dataset.platform = platform;

function selected(): ProfileStatus | null {
  return state.profiles.find((status) => status.profile.id === state.selectedId) ?? null;
}

function visible(): ProfileStatus[] {
  return filterProfiles(sortProfiles(state.profiles), state.filter);
}

function select(id: string | null): void {
  state.selectedId = id;
  render();
}

async function refresh(): Promise<void> {
  try {
    state.profiles = await listProfiles();
    state.fatal = null;
  } catch (error) {
    state.fatal = error as CdmError;
    render();
    return;
  }
  const ids = new Set(state.profiles.map((status) => status.profile.id));
  for (const id of state.missing) if (!ids.has(id)) state.missing.delete(id);
  if (!state.selectedId || !ids.has(state.selectedId)) {
    state.selectedId = visible()[0]?.profile.id ?? null;
  }
  render();
}

async function discover(): Promise<void> {
  state.candidates = await listAdoptable().catch(() => []);
  render();
}

function render(): void {
  const active = document.activeElement as HTMLElement | null;
  const previous = active?.dataset?.focusKey;
  const caret = active instanceof HTMLInputElement ? active.selectionStart : null;
  // Focus follows the selection, so a keyboard move lands on the row it just selected.
  const focusKey = previous?.startsWith("row-") ? `row-${state.selectedId}` : previous;

  root.replaceChildren(...panes());

  if (focusKey) {
    const restored = root.querySelector<HTMLElement>(`[data-focus-key="${focusKey}"]`);
    restored?.focus();
    if (restored instanceof HTMLInputElement && caret !== null) {
      restored.setSelectionRange(caret, caret);
    }
  }
}

function panes(): HTMLElement[] {
  const tabs = renderTabs({ active: state.tab, onSelect: selectTab });
  return [tabs, state.tab === "updates" ? updatesPane() : profilesPane()];
}

function selectTab(tab: TabId): void {
  state.tab = tab;
  render();
  root.querySelector<HTMLElement>(`[data-focus-key="tab-${tab}"]`)?.focus();
}

function pane(kind: string, children: HTMLElement[]): HTMLElement {
  const element = document.createElement("div");
  element.className = `pane pane-${kind}`;
  element.setAttribute("role", "tabpanel");
  element.append(...children);
  return element;
}

function profilesPane(): HTMLElement {
  if (state.fatal) {
    return pane("plain", [
      renderRegistryError(state.fatal, () => void refresh(), () => void refresh()),
    ]);
  }
  if (state.profiles.length === 0) {
    return pane("plain", [
      renderEmpty({ adoptable: state.candidates.length > 0, onNew: newProfile, onAdopt: adopt }),
    ]);
  }
  return pane("profiles", manager());
}

function updatesPane(): HTMLElement {
  return pane("settings", [
    renderUpdates({
      version: state.version,
      state: state.update,
      onCheck: runUpdateCheck,
      onUpdate: runUpdateInstall,
    }),
  ]);
}

function runUpdateCheck(): void {
  if (state.update.phase === "checking" || state.update.phase === "installing") return;
  state.update = { phase: "checking" };
  render();

  void checkForUpdates()
    .then((outcome) => {
      state.update =
        outcome.status === "available"
          ? { phase: "available", version: outcome.version }
          : { phase: "upToDate", version: outcome.version };
    })
    .catch((error: CdmError) => {
      state.update = { phase: "failed", step: "check", detail: error.message };
    })
    .finally(settleUpdate);
}

function runUpdateInstall(): void {
  if (state.update.phase !== "available") return;
  state.update = { phase: "installing", version: state.update.version };
  render();

  void installUpdate()
    .then((version) => {
      state.update = { phase: "installed", version };
    })
    .catch((error: CdmError) => {
      state.update = { phase: "failed", step: "install", detail: error.message };
    })
    .finally(settleUpdate);
}

/** Focus lands on whatever the pane now offers: Update when one is waiting, otherwise Check. */
function settleUpdate(): void {
  render();
  const next =
    root.querySelector<HTMLElement>('[data-focus-key="install-update"]') ??
    root.querySelector<HTMLElement>('[data-focus-key="check-updates"]');
  next?.focus();
}

function manager(): HTMLElement[] {
  const status = selected();
  const parts: HTMLElement[] = [
    renderToolbar(
      { hasSelection: !!state.selectedId, hasCandidates: state.candidates.length > 0 },
      actions,
    ),
    renderSidebar({
      profiles: state.profiles,
      selectedId: state.selectedId,
      missingIds: state.missing,
      filter: state.filter,
      onSelect: select,
      onActivate: launch,
      onFilter: (value) => {
        state.filter = value;
        render();
      },
      onContextMenu: (id, x, y) => openMenu(rowMenu(id, actions), { x, y }),
    }),
    renderDetail({
      status,
      launching: !!status && state.launching.has(status.profile.id),
      missing: !!status && state.missing.has(status.profile.id),
      actions: {
        launch: () => {
          if (status) launch(status.profile.id);
        },
        rename,
        editConfig,
        reveal,
        remove,
        locate,
        removeFromList,
      },
    }),
  ];
  if (state.candidates.length > 0 && !state.bannerDismissed) {
    parts.push(
      renderAdoptBanner(state.candidates.length, adopt, () => {
        state.bannerDismissed = true;
        render();
      }),
    );
  }
  return parts;
}

function newProfile(): void {
  openCreateSheet({
    existingNames: state.profiles.map((status) => status.profile.name),
    onCreated: (profile: Profile) => {
      state.selectedId = profile.id;
      void refresh().then(() => {
        root.querySelector<HTMLElement>('[data-focus-key="primary"]')?.focus();
      });
    },
  });
}

function launch(id: string): void {
  const status = state.profiles.find((entry) => entry.profile.id === id);
  if (!status) return;
  state.launching.add(id);
  render();
  window.setTimeout(() => {
    state.launching.delete(id);
    render();
  }, LAUNCH_FEEDBACK_MS);

  void launchProfile(id)
    .then(() => refresh())
    .catch((error: CdmError) => {
      state.launching.delete(id);
      if (error.kind === "BinaryNotFound") {
        render();
        showBinaryNotFound(() => launch(id));
        return;
      }
      if (error.kind === "ProfileNotFound") {
        state.missing.add(id);
        select(id);
        return;
      }
      render();
      showLaunchFailed(error, {
        operation: "launch",
        profile: status.profile,
        onRetry: () => launch(id),
      });
    });
}

function rename(): void {
  const status = selected();
  if (!status) return;
  openRenameSheet({ status, onRenamed: () => void refresh() });
}

function remove(): void {
  const status = selected();
  if (!status) return;
  const order = visible();
  const index = order.findIndex((entry) => entry.profile.id === status.profile.id);
  confirmDelete({
    status,
    onDeleted: () => {
      const next = order[index + 1] ?? order[index - 1];
      state.selectedId = next ? next.profile.id : null;
      void refresh();
    },
  });
}

function removeFromList(): void {
  const status = selected();
  if (!status) return;
  confirmRemoveFromList({
    profile: status.profile,
    onRemoved: () => {
      state.missing.delete(status.profile.id);
      state.selectedId = null;
      void refresh();
    },
  });
}

// The picker and its four refusals (§3.7) belong to the command, which owns the file dialog.
function locate(): void {
  const status = selected();
  if (!status) return;
  void locateFolder(status.profile.id)
    .then(() => {
      state.missing.delete(status.profile.id);
      return refresh();
    })
    .catch(() => undefined);
}

function editConfig(): void {
  const status = selected();
  if (!status) return;
  void openConfig(status.profile.id).catch((error: CdmError) =>
    reportError(error, { operation: "config", profile: status.profile }),
  );
}

function reveal(): void {
  const status = selected();
  if (!status) return;
  void revealProfile(status.profile.id).catch((error: CdmError) =>
    reportError(error, { operation: "reveal", profile: status.profile }),
  );
}

function adopt(): void {
  if (state.candidates.length === 0) return;
  openAdoptSheet({
    candidates: state.candidates,
    onAdopted: () => {
      state.bannerDismissed = true;
      void refresh().then(discover);
    },
  });
}

const actions: CommandActions = {
  newProfile,
  launch,
  rename,
  editConfig,
  reveal,
  remove,
  adopt,
  copyDiagnostics: () => void copyDiagnostics(),
};

function onKeydown(event: KeyboardEvent): void {
  if (isDialogOpen()) return;
  const target = event.target as HTMLElement;
  const typing = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;

  if (matches(event, shortcuts.newProfile)) {
    event.preventDefault();
    // Creating from the Updates tab has to land somewhere visible.
    state.tab = "profiles";
    newProfile();
  } else if (matches(event, shortcuts.hideWindow)) {
    event.preventDefault();
    hideWindow();
  } else if (state.tab !== "profiles") {
    return;
  } else if (!typing && matches(event, shortcuts.reveal)) {
    event.preventDefault();
    reveal();
  } else if (!typing && matches(event, shortcuts.rename)) {
    event.preventDefault();
    rename();
  } else if (!typing && matches(event, shortcuts.delete)) {
    event.preventDefault();
    remove();
  } else if (!typing && matches(event, shortcuts.editConfig)) {
    event.preventDefault();
    editConfig();
  }
}

/** §6 — the window is shown from the tray: land on the list with the previous row selected. */
function focusEntry(): void {
  if (state.tab !== "profiles") {
    root.querySelector<HTMLElement>(`[data-focus-key="tab-${state.tab}"]`)?.focus();
    return;
  }
  const row = root.querySelector<HTMLElement>('[data-focus-key^="row-"][aria-selected="true"]');
  (row ?? root.querySelector<HTMLElement>('[data-focus-key="new-profile"]'))?.focus();
}

/** The tray shows the version too; both read the one in `tauri.conf.json`. */
async function loadVersion(): Promise<void> {
  state.version = await appVersion();
  render();
}

document.addEventListener("keydown", onKeydown);
onWindowShown(() => {
  if (isDialogOpen()) return;
  void refresh().then(focusEntry);
});
onTrayEvent("locateBinary", () => {
  selectTab("profiles");
  showBinaryNotFound(() => void refresh());
});

void refresh().then(() => {
  focusEntry();
  return Promise.all([discover(), loadVersion()]);
});
