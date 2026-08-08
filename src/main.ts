import {
  appVersion,
  checkForUpdates,
  getGeneralSettings,
  hideWindow,
  installUpdate,
  isTranslated,
  launchProfile,
  listAdoptable,
  listGroups,
  listProfiles,
  locateFolder,
  moveProfile,
  onTrayEvent,
  onUpdateProgress,
  onWindowShown,
  openConfig,
  restartApp,
  revealProfile,
  setGroupIcon,
  setLaunchAtLogin,
  setOpenPreferencesAtStart,
  setShowUsageLimits,
  setTheme,
  type AdoptCandidate,
  type CdmError,
  type GeneralSettings,
  type Group,
  type Profile,
  type ProfileStatus,
  type Theme,
  type UpdateProgress,
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
  showNotice,
  showTranslatedBuild,
} from "./views/errors";
import { renderGeneral } from "./views/general";
import {
  openAssignGroupSheet,
  openDeleteGroupSheet,
  openNewGroupSheet,
  openRenameGroupSheet,
} from "./views/groups";
import { openIconPicker } from "./views/icon-picker";
import { filterProfiles, ordered, renderSidebar } from "./views/list";
import { openMenu, type MenuEntry } from "./views/menu";
import { matches, platform, shortcuts } from "./views/platform";
import { openRenameSheet } from "./views/rename";
import { restoreSidebarWidth } from "./views/resize";
import { t } from "./views/strings";
import { renderTabs, type TabId } from "./views/tabs";
import { applyTheme } from "./views/theme";
import { renderToolbar, rowMenu, type CommandActions } from "./views/toolbar";
import { RateMeter } from "./views/transfer";
import {
  isBusy,
  isPending,
  paintDownload,
  renderUpdates,
  type Downloading,
  type UpdateState,
} from "./views/updates";

const LAUNCH_FEEDBACK_MS = 3000;
const UPDATE_POLL_MS = 60 * 60 * 1000;

const NO_BADGES: ReadonlySet<TabId> = new Set();
const UPDATE_BADGE: ReadonlySet<TabId> = new Set(["updates"]);

const root = document.getElementById("app") as HTMLElement;

const state = {
  tab: "profiles" as TabId,
  profiles: [] as ProfileStatus[],
  groups: [] as Group[],
  order: [] as string[],
  groupsUnavailable: false,
  candidates: [] as AdoptCandidate[],
  selectedId: null as string | null,
  filter: "",
  collapsed: new Set<string>(),
  launching: new Set<string>(),
  missing: new Set<string>(),
  bannerDismissed: false,
  fatal: null as CdmError | null,
  version: "",
  update: { phase: "idle" } as UpdateState,
  settings: {
    openPreferencesAtStart: true,
    launchAtLogin: false,
    showUsageLimits: true,
    theme: "system",
  } as GeneralSettings,
  settingsError: null as string | null,
};

document.documentElement.dataset.platform = platform;
// The stored choice arrives over IPC; until then the system's own appearance is the best guess.
applyTheme(state.settings.theme);

function selected(): ProfileStatus | null {
  return state.profiles.find((status) => status.profile.id === state.selectedId) ?? null;
}

function visible(): ProfileStatus[] {
  return filterProfiles(ordered(state.profiles, state.order), state.filter);
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
  // Groups are cosmetic: an unreadable groups file must not break the profile list. But a
  // failed read is not "no groups" — reordering from that view would send groupId: null and
  // strip memberships the user still has.
  const groups = await listGroups().catch(() => null);
  state.groupsUnavailable = groups === null;
  state.groups = groups?.groups ?? [];
  state.order = groups?.order ?? [];
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
  // Focus follows the selection, so a keyboard move lands on what it just chose.
  const focusKey = previous?.startsWith("row-")
    ? `row-${state.selectedId}`
    : previous?.startsWith("theme-")
      ? `theme-${state.settings.theme}`
      : previous;

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
  const tabs = renderTabs({
    active: state.tab,
    badges: isPending(state.update) ? UPDATE_BADGE : NO_BADGES,
    onSelect: selectTab,
  });
  return [tabs, activePane()];
}

function activePane(): HTMLElement {
  switch (state.tab) {
    case "updates":
      return updatesPane();
    case "general":
      return generalPane();
    default:
      return profilesPane();
  }
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
      onRestart: runRestart,
    }),
  ]);
}

function generalPane(): HTMLElement {
  return pane("settings", [
    renderGeneral({
      openPreferencesAtStart: state.settings.openPreferencesAtStart,
      launchAtLogin: state.settings.launchAtLogin,
      showUsageLimits: state.settings.showUsageLimits,
      theme: state.settings.theme,
      error: state.settingsError,
      onTheme: (theme: Theme) => {
        state.settings.theme = theme;
        applyTheme(theme);
        void store(setTheme(theme));
      },
      onOpenPreferencesAtStart: (enabled) => {
        state.settings.openPreferencesAtStart = enabled;
        void store(setOpenPreferencesAtStart(enabled));
      },
      onLaunchAtLogin: (enabled) => {
        state.settings.launchAtLogin = enabled;
        void store(setLaunchAtLogin(enabled));
      },
      onShowUsageLimits: (enabled) => {
        state.settings.showUsageLimits = enabled;
        void store(setShowUsageLimits(enabled));
      },
    }),
  ]);
}

async function loadSettings(): Promise<void> {
  state.settings = await getGeneralSettings().catch(() => state.settings);
  applyTheme(state.settings.theme);
  render();
}

/** The box the user just clicked already shows the new value; a refusal reads the truth back. */
async function store(pending: Promise<void>): Promise<void> {
  try {
    await pending;
    state.settingsError = null;
    render();
  } catch (error) {
    state.settingsError = `${t.general.saveFailed} ${(error as CdmError).message}`;
    await loadSettings();
  }
}

function runUpdateCheck(): void {
  if (isBusy(state.update)) return;
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

/**
 * Nobody asked for this one, so it stays quiet: no "Checking…" in the pane, and a failure is
 * dropped rather than shown. It speaks up only to report a version worth having — leaving the
 * phase untouched otherwise, so the next hour tries again.
 */
function pollForUpdate(): void {
  if (isBusy(state.update) || isPending(state.update)) return;

  void checkForUpdates()
    .then((outcome) => {
      if (outcome.status !== "available") return;
      state.update = { phase: "available", version: outcome.version };
      render();
    })
    .catch(() => {});
}

/** Replaced per install: the speed shown is this download's, not a previous attempt's. */
let downloadRate = new RateMeter();

function runUpdateInstall(): void {
  if (state.update.phase !== "available") return;
  downloadRate = new RateMeter();
  state.update = {
    phase: "downloading",
    version: state.update.version,
    downloaded: 0,
    total: null,
    speed: null,
  };
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

/** The guard also drops events from a download that already finished, failed, or was superseded. */
function onProgress(progress: UpdateProgress): void {
  if (state.update.phase !== "downloading") return;

  if (progress.step === "unpacking") {
    state.update = { phase: "unpacking", version: state.update.version };
    render();
    return;
  }

  downloadRate.record(progress.downloaded);
  const next: Downloading = {
    ...state.update,
    downloaded: progress.downloaded,
    total: progress.total,
    speed: downloadRate.perSecond(),
  };
  state.update = next;
  paintDownload(root, next);
}

/**
 * The command only returns if the restart never happened — on success this process is already
 * gone, webview and all.
 */
function runRestart(): void {
  if (state.update.phase !== "installed") return;
  state.update = { phase: "restarting", version: state.update.version };
  render();

  void restartApp().catch((error: CdmError) => {
    state.update = { phase: "failed", step: "restart", detail: error.message };
    settleUpdate();
  });
}

/** Focus lands on whatever the pane now offers, in the order it asks to be acted on. */
function settleUpdate(): void {
  render();
  const next =
    root.querySelector<HTMLElement>('[data-focus-key="restart-app"]') ??
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
      groups: state.groups,
      order: state.order,
      collapsed: state.collapsed,
      selectedId: state.selectedId,
      missingIds: state.missing,
      filter: state.filter,
      reorderable: !state.groupsUnavailable,
      showUsage: state.settings.showUsageLimits,
      onSelect: select,
      onActivate: launch,
      onFilter: (value) => {
        state.filter = value;
        render();
      },
      onContextMenu: (id, x, y) => openMenu(rowMenu(id, actions), { x, y }),
      onToggleGroup: (id) => {
        if (state.collapsed.has(id)) state.collapsed.delete(id);
        else state.collapsed.add(id);
        render();
      },
      onGroupMenu: (id, x, y) => openMenu(groupMenu(id), { x, y }),
      onMove: (id, groupId, before) => void reorderProfile(id, groupId, before),
    }),
    renderDetail({
      status,
      launching: !!status && state.launching.has(status.profile.id),
      missing: !!status && state.missing.has(status.profile.id),
      showUsage: state.settings.showUsageLimits,
      actions: {
        launch: () => {
          if (status) launch(status.profile.id);
        },
        rename,
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

function newGroup(): void {
  openNewGroupSheet({ onCreated: () => void refresh() });
}

function assignToGroup(): void {
  const status = selected();
  if (!status) return;
  const current = state.groups.find((group) => group.profileIds.includes(status.profile.id));
  openAssignGroupSheet({
    profileId: status.profile.id,
    groups: state.groups,
    currentGroupId: current?.id ?? null,
    onAssigned: () => void refresh(),
  });
}

async function reorderProfile(
  id: string,
  groupId: string | null,
  before: string | null,
): Promise<void> {
  try {
    await moveProfile(id, groupId, before);
    void refresh();
  } catch (error) {
    showNotice(t.list.moveFailed, (error as CdmError).message);
  }
}

function groupMenu(id: string): MenuEntry[] {
  const group = state.groups.find((entry) => entry.id === id);
  if (!group) return [];
  return [
    { label: t.groups.rename, onSelect: () => openRenameGroupSheet({ group, onRenamed: () => void refresh() }) },
    { label: t.groups.chooseIcon, onSelect: () => chooseGroupIcon(group.id) },
    "separator",
    {
      label: t.groups.delete,
      destructive: true,
      onSelect: () =>
        openDeleteGroupSheet({ group, onDeleted: () => void refresh() }),
    },
  ];
}

function chooseGroupIcon(id: string): void {
  const group = state.groups.find((entry) => entry.id === id);
  if (!group) return;
  openIconPicker({
    current: group.icon,
    onSelect: (icon) => {
      void setGroupIcon(id, icon)
        .then(refresh)
        .catch((error: CdmError) => showNotice(t.groups.iconFailed, error.message));
    },
  });
}

const actions: CommandActions = {
  newProfile,
  newGroup,
  launch,
  rename,
  assignToGroup,
  editConfig,
  reveal,
  remove,
  adopt,
  copyDiagnostics: () => void copyDiagnostics(),
  refresh: () => void refresh(),
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

async function warnIfTranslated(): Promise<void> {
  if (await isTranslated().catch(() => false)) showTranslatedBuild();
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
onUpdateProgress(onProgress);

pollForUpdate();
setInterval(pollForUpdate, UPDATE_POLL_MS);

void refresh().then(() => {
  focusEntry();
  return Promise.all([
    discover(),
    loadVersion(),
    loadSettings(),
    restoreSidebarWidth(),
    warnIfTranslated(),
  ]);
});
