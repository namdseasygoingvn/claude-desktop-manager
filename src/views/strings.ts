import { isMac } from "./platform";

/** The only per-platform string difference the design allows. */
export const nouns = {
  trash: isMac ? "Trash" : "Recycle Bin",
  fileManager: isMac ? "Finder" : "File Explorer",
  revealButton: isMac ? "Reveal…" : "Show…",
  revealItem: isMac ? "Reveal in Finder" : "Show in File Explorer",
  machine: isMac ? "this Mac" : "this PC",
  profilesRoot: isMac ? "Application Support" : "the Roaming folder",
  tray: isMac ? "menu bar" : "system tray",
  signIn: isMac ? "log in" : "sign in",
};

export function q(name: string): string {
  return `“${name}”`;
}

const MINUTE = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;

function time(date: Date): string {
  return date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

function day(date: Date, now: Date): string {
  if (date.toDateString() === now.toDateString()) return "today";
  if (now.getTime() - date.getTime() < 7 * DAY) {
    return date.toLocaleDateString(undefined, { weekday: "long" });
  }
  return date.toLocaleDateString(undefined, { day: "numeric", month: "long" });
}

export function lastUsedShort(iso: string): string {
  const date = new Date(iso);
  return `Last used ${day(date, new Date())}`;
}

export function lastUsedLong(iso: string, lowercase = false): string {
  const date = new Date(iso);
  return `${lowercase ? "last" : "Last"} used ${day(date, new Date())} at ${time(date)}`;
}

export function sampleAge(epochMs: number): string {
  const elapsed = Date.now() - epochMs;
  if (elapsed < MINUTE) return "just now";
  if (elapsed < HOUR) return `${Math.floor(elapsed / MINUTE)}m ago`;
  if (elapsed < DAY) return `${Math.floor(elapsed / HOUR)}h ago`;
  return `${Math.floor(elapsed / DAY)}d ago`;
}

export function createdLine(iso: string): string {
  const date = new Date(iso);
  return `Created ${date.toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" })}`;
}

const trashRecovery = isMac
  ? "Everything is moved to the Trash, so you can put it back until you empty the Trash."
  : "Everything is moved to the Recycle Bin, so you can restore it until you empty the Recycle Bin.";

const deleteLoss =
  "You'll be signed out of Claude in this profile, and its chats, MCP servers and extensions go with it.";

const appName = "Claude Desktop Manager";

export const t = {
  appName,

  tabs: {
    label: "Preferences sections",
    profiles: "Profiles",
    updates: "Updates",
    general: "General",
  },

  general: {
    heading: "General",
    theme: "Theme",
    themeHint: "Match the system appearance, or pick one.",
    themes: { light: "Light", dark: "Dark", system: "System" },
    openAtStart: "Open Preferences at start",
    openAtStartHint: `Uncheck to start straight in the ${nouns.tray}, with no window.`,
    launchAtLogin: "Open with System startup",
    launchAtLoginHint: `Start ${appName} automatically when you ${nouns.signIn} to ${nouns.machine}.`,
    saveFailed: "Couldn't save that setting.",
  },

  updates: {
    heading: "Updates",
    version: (version: string) => `Version ${version}`,
    check: "Check for Updates",
    checking: "Checking…",
    upToDate: (version: string) => `You're up to date! Version is ${version}`,
    available: (version: string) =>
      `Version ${version} is ready to install! Click Update to install`,
    update: "Update",
    installing: "Installing…",
    installed: (version: string) =>
      `Version ${version} is installed. Restart ${appName} to start using it.`,
    restart: "Restart Now",
    restarting: "Restarting…",
    installedHint: `Your running profiles stay open. Skip the restart and the update applies the next time you open ${appName}.`,
    failed: {
      check: "Couldn't check for updates.",
      install: "Couldn't install the update.",
      restart: `Couldn't restart ${appName}.`,
    },
  },

  empty: {
    heading: "No profiles yet",
    body: "A profile is a separate Claude Desktop — its own login, its own MCP servers, its own chats. Nothing you already have in Claude Desktop is changed or moved.",
    primary: "New Profile",
    adoptLink: "Already have a folder? Add it…",
  },

  list: {
    header: "Profiles",
    filterLabel: "Filter profiles",
    filterPlaceholder: "Filter",
    running: "Running",
    neverLaunched: "Never launched",
    missing: "Missing",
    reorderHint: "Drag to reorder or move to a group",
    moveFailed: "Couldn't move the profile.",
    newProfile: "New Profile",
    deleteProfile: "Delete Profile",
    moreActions: "More Actions",
    addExisting: "Add Existing Folder…",
    copyDiagnostics: "Copy Diagnostics",
    rowLabel: (name: string, running: boolean, usage?: string | null) => {
      const base = running ? `${name}, running` : name;
      return usage ? `${base}, ${usage}` : base;
    },
  },

  groups: {
    ungrouped: "Ungrouped",
    newGroup: "New Group…",
    createTitle: "New Group",
    createNameLabel: "Name",
    createPlaceholder: "Work",
    createSubmit: "Create",
    createFailed: (name: string) => `Couldn't create the group ${q(name)}.`,
    assignToGroup: "Assign to Group…",
    assignTitle: "Assign to Group",
    assignNone: "No group",
    assignSubmit: "Assign",
    assignFailed: "Couldn't change the group.",
    rename: "Rename Group…",
    renameTitle: "Rename Group",
    renameSubmit: "Rename",
    renameFailed: (name: string) => `Couldn't rename the group ${q(name)}.`,
    chooseIcon: "Choose Icon…",
    delete: "Delete Group…",
    deleteMessage: (name: string) => `Delete the group ${q(name)}?`,
    deleteInformative:
      "The profiles in it stay in your profile list — only the group goes away.",
    deleteConfirm: "Delete",
    deleteFailed: (name: string) => `Couldn't delete the group ${q(name)}.`,
    iconFailed: "Couldn't save the icon.",
    empty: "No profiles in this group",
    picker: {
      title: "Choose Icon",
      emoji: "Emoji",
      icons: "Icons",
      search: "Search icons",
      remove: "Remove Icon",
    },
  },

  detail: {
    neverLaunched: "Never launched · not signed in yet",
    neverLaunchedHint:
      "Launch this profile and sign in to Claude. It won't affect any other profile.",
    running: (iso: string | null) => (iso ? `Running · ${lastUsedLong(iso, true)}` : "Running"),
    idle: (iso: string) => lastUsedLong(iso),
    starting: "Starting…",
    launch: "Launch",
    launching: "Launching…",
    rename: "Rename…",
    editConfig: "Edit MCP Config…",
    reveal: nouns.revealButton,
    delete: "Delete Profile…",
    created: createdLine,
  },

  usage: {
    fiveHour: "5-hour limit",
    weekly: "Weekly · all models",
    fiveHourShort: "5h",
    weeklyShort: "7d",
    show: "Show usage limits",
    showHint: `Show each profile's 5-hour and weekly usage in the ${nouns.tray} and in this window.`,
    refresh: "Refresh usage",
    age: sampleAge,
  },

  orphan: {
    secondLine: "Folder missing",
    body: `${appName} can't find this profile's folder in ${nouns.profilesRoot}. It may have been moved, renamed, or deleted outside the app. Its login and chats live in that folder — ${appName} has no copy of them.`,
    locate: "Locate Folder…",
    remove: "Remove from List",
    belongsToOther: (name: string) => `That folder belongs to the profile ${q(name)}.`,
    outsideRootMessage: `Profiles have to live in ${nouns.profilesRoot}. Move the folder there and try again.`,
    revealFolder: "Reveal Folder",
    removeMessage: (name: string) => `Remove ${q(name)} from your profile list?`,
    removeInformative: `This only removes it from ${appName}. Nothing on your disk is deleted. If the folder turns up later you can add it back.`,
    removeConfirm: "Remove",
  },

  create: {
    title: "New Profile",
    nameLabel: "Name",
    placeholder: "Work",
    helper: "You'll sign in to Claude the first time you launch this profile.",
    submit: "Create",
    duplicate: (name: string) =>
      `You already have a profile named ${q(name)}. That's allowed — they'll be separate.`,
    failedMessage: (name: string) => `Couldn't create ${q(name)}.`,
    failedFallback: `${appName} couldn't finish creating the profile.`,
  },

  rename: {
    title: "Rename Profile",
    submit: "Rename",
    submitRunning: "Quit & Rename",
    quitting: "Quitting…",
    runningWarning: (name: string) =>
      `${q(name)} is running and has to quit before it can be renamed. Your chats are saved; anything in progress will stop.`,
    failedMessage: (name: string) => `Couldn't rename ${q(name)}.`,
    failedInformative: `${appName} couldn't save the change to your profile list.`,
  },

  remove: {
    message: (name: string) => `Delete the profile ${q(name)}?`,
    informative: `${deleteLoss} ${trashRecovery}`,
    runningMessage: (name: string) => `Quit Claude and delete the profile ${q(name)}?`,
    runningInformative: (name: string) =>
      `${q(name)} is running and has to quit first. ${deleteLoss} ${trashRecovery}`,
    confirm: "Delete",
    confirmRunning: "Quit & Delete",
    trashFailedMessage: (name: string) =>
      `Couldn't move ${q(name)} to the ${nouns.trash}.`,
    trashFailedInformative: `It can still be deleted, but it won't be recoverable — there'll be nothing in the ${nouns.trash} to put back.`,
    deletePermanently: "Delete Permanently",
    failedMessage: (name: string) => `Couldn't delete ${q(name)}.`,
    failedInformative:
      "Some of its files are still in use. Make sure Claude isn't running for this profile, then try again.",
    partialMessage: (name: string) => `${q(name)} was only partly deleted.`,
    partialInformative: `Some of its files couldn't be removed. ${appName} has left the profile in your list so you can try again.`,
  },

  adopt: {
    title: "Add Existing Profiles",
    body: `These folders look like Claude Desktop profiles. Adding one lets ${appName} launch it. It puts one small marker file in each folder you add — nothing else is changed, and nothing is moved.`,
    nameLabel: "Name",
    submit: (count: number) => (count === 1 ? "Add Profile" : `Add ${count} Profiles`),
    banner: (count: number) =>
      count === 1
        ? "1 folder here looks like a Claude profile."
        : `${count} folders here look like Claude profiles.`,
    bannerAction: "Review…",
    dismiss: "Dismiss",
  },

  binary: {
    message: "Can't find Claude Desktop.",
    informative: `${appName} launches the copy of Claude Desktop already installed on ${nouns.machine}, and couldn't find one.`,
    locate: "Locate Claude Desktop…",
    get: "Get Claude Desktop",
    wrongPickMessage: "That doesn't look like Claude Desktop.",
    wrongPickInformative: "Pick the Claude app in your Applications folder.",
  },

  rosetta: {
    message: `This is the Intel build of ${appName}.`,
    informative: `It's running through Rosetta on ${nouns.machine}, and every profile it launches inherits that — Claude Desktop then takes several seconds to answer each keystroke and click. The Apple silicon build fixes it. Updating in place won't, so that build has to be downloaded by hand.`,
    download: "Get the Apple Silicon Build",
    dismiss: "Continue Anyway",
  },

  launch: {
    failedMessage: (name: string) => `Couldn't launch ${q(name)}.`,
    failedFallback: "Claude Desktop closed straight away.",
  },

  quit: {
    stuckMessage: (name: string) => `${q(name)} isn't quitting.`,
    stuckInformative: `${appName} asked Claude to quit and it hasn't. You can force it to close, but anything in progress will be lost.`,
    force: "Force Quit",
  },

  registry: {
    corruptHeading: `${appName} can't read your profile list`,
    corruptBody:
      "The file that stores your profile names is damaged. Your profiles themselves are untouched — it just can't tell which is which.",
    corruptRebuild:
      "Rebuilding finds your profiles again. Some names may come back looking different, and you can rename them.",
    rebuild: "Rebuild List",
    showDamaged: "Show the Damaged File",
    unreadableHeading: `${appName} can't open your profile list`,
    unreadableBody: `Something is stopping ${appName} from reading its own settings.`,
    showFile: "Show the File",
  },

  common: {
    couldntOpen: (name: string) => `Couldn't open ${q(name)}.`,
    cancel: "Cancel",
    ok: "OK",
    tryAgain: "Try Again",
    copyDetails: "Copy Details",
    copied: "Details copied.",
  },
};
