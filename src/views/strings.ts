import { isMac } from "./platform";

/** The only per-platform string difference the design allows. */
export const nouns = {
  trash: isMac ? "Trash" : "Recycle Bin",
  fileManager: isMac ? "Finder" : "File Explorer",
  revealButton: isMac ? "Reveal…" : "Show…",
  revealItem: isMac ? "Reveal in Finder" : "Show in File Explorer",
  machine: isMac ? "this Mac" : "this PC",
  profilesRoot: isMac ? "Application Support" : "the Roaming folder",
};

export function q(name: string): string {
  return `“${name}”`;
}

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

export function createdLine(iso: string): string {
  const date = new Date(iso);
  return `Created ${date.toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" })}`;
}

const trashRecovery = isMac
  ? "Everything is moved to the Trash, so you can put it back until you empty the Trash."
  : "Everything is moved to the Recycle Bin, so you can restore it until you empty the Recycle Bin.";

const deleteLoss =
  "You'll be signed out of Claude in this profile, and its chats, MCP servers and extensions go with it.";

export const t = {
  appName: "Claude Desktop Manager",

  tabs: {
    label: "Preferences sections",
    profiles: "Profiles",
    updates: "Updates",
  },

  updates: {
    heading: "Updates",
    version: (version: string) => `Version ${version}`,
    check: "Check for Updates",
    checking: "Checking…",
    upToDate: "cdm is up to date.",
    installed: (version: string) =>
      `Version ${version} is installed. It starts running the next time you open cdm.`,
    // The running process keeps the old code on purpose: restarting would kill the profiles it spawned.
    installedHint: "Your running profiles are untouched.",
    failed: "Couldn't check for updates.",
    auto: "cdm checks for updates on its own every few hours.",
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
    newProfile: "New Profile",
    deleteProfile: "Delete Profile",
    moreActions: "More Actions",
    addExisting: "Add Existing Folder…",
    copyDiagnostics: "Copy Diagnostics",
    rowLabel: (name: string, running: boolean) => (running ? `${name}, running` : name),
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

  orphan: {
    secondLine: "Folder missing",
    body: `cdm can't find this profile's folder in ${nouns.profilesRoot}. It may have been moved, renamed, or deleted outside cdm. Its login and chats live in that folder — cdm has no copy of them.`,
    locate: "Locate Folder…",
    remove: "Remove from List",
    belongsToOther: (name: string) => `That folder belongs to the profile ${q(name)}.`,
    outsideRootMessage: `Profiles have to live in ${nouns.profilesRoot}. Move the folder there and try again.`,
    revealFolder: "Reveal Folder",
    removeMessage: (name: string) => `Remove ${q(name)} from your profile list?`,
    removeInformative:
      "This only removes it from cdm. Nothing on your disk is deleted. If the folder turns up later you can add it back.",
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
    failedFallback: "cdm couldn't finish creating the profile.",
  },

  rename: {
    title: "Rename Profile",
    submit: "Rename",
    submitRunning: "Quit & Rename",
    quitting: "Quitting…",
    runningWarning: (name: string) =>
      `${q(name)} is running and has to quit before it can be renamed. Your chats are saved; anything in progress will stop.`,
    failedMessage: (name: string) => `Couldn't rename ${q(name)}.`,
    failedInformative: "cdm couldn't save the change to your profile list.",
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
    partialInformative:
      "Some of its files couldn't be removed. cdm has left the profile in your list so you can try again.",
  },

  adopt: {
    title: "Add Existing Profiles",
    body: "These folders look like Claude Desktop profiles. Adding one lets cdm launch it. cdm puts one small marker file in each folder you add — nothing else is changed, and nothing is moved.",
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
    informative: `cdm launches the copy of Claude Desktop already installed on ${nouns.machine}, and couldn't find one.`,
    locate: "Locate Claude Desktop…",
    get: "Get Claude Desktop",
    wrongPickMessage: "That doesn't look like Claude Desktop.",
    wrongPickInformative: "Pick the Claude app in your Applications folder.",
  },

  launch: {
    failedMessage: (name: string) => `Couldn't launch ${q(name)}.`,
    failedFallback: "Claude Desktop closed straight away.",
  },

  quit: {
    stuckMessage: (name: string) => `${q(name)} isn't quitting.`,
    stuckInformative:
      "cdm asked Claude to quit and it hasn't. You can force it to close, but anything in progress will be lost.",
    force: "Force Quit",
  },

  registry: {
    corruptHeading: "cdm can't read your profile list",
    corruptBody:
      "The file that stores your profile names is damaged. Your profiles themselves are untouched — cdm just can't tell which is which.",
    corruptRebuild:
      "Rebuilding finds your profiles again. Some names may come back looking different, and you can rename them.",
    rebuild: "Rebuild List",
    showDamaged: "Show the Damaged File",
    unreadableHeading: "cdm can't open your profile list",
    unreadableBody: "Something is stopping cdm from reading its own settings.",
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
