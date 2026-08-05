import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Emitted by the tray, which shows Preferences first and then asks it to open a sheet. */
export const TRAY_EVENTS = {
  locateBinary: "cdm://locate-binary",
} as const;

export interface Profile {
  id: string;
  name: string;
  dir: string;
  createdAt: string;
  lastUsedAt: string | null;
}

export interface ProfileStatus {
  profile: Profile;
  runningPid: number | null;
}

export interface AdoptCandidate {
  dirName: string;
  suggestedName: string;
}

/** The check only looks; installing is a separate, explicit step. */
export type UpdateOutcome =
  | { status: "upToDate"; version: string }
  | { status: "available"; version: string };

/** Owned by the backend; the frontend only ever stringifies it into Copy Details. */
export type DoctorReport = Record<string, unknown>;

export const ERROR_KINDS = [
  "BinaryNotFound",
  "ProfileNotFound",
  "ProfileRunning",
  "NameEmpty",
  "DirExists",
  "RegistryCorrupt",
  "Io",
  "Other",
] as const;

export type CdmErrorKind = (typeof ERROR_KINDS)[number];

export interface CdmError {
  kind: CdmErrorKind;
  message: string;
  raw: unknown;
}

const KINDS: readonly string[] = ERROR_KINDS;
const BY_TOKEN = new Map(ERROR_KINDS.map((kind) => [kind.toLowerCase(), kind]));

/** The wire token may be camelCase or PascalCase depending on how the variant is serialized. */
function kindOf(value: string): CdmErrorKind | null {
  return BY_TOKEN.get(value.toLowerCase()) ?? null;
}

/** serde can emit a bare string, an externally tagged object, or a struct; accept all of them. */
export function toCdmError(raw: unknown): CdmError {
  if (typeof raw === "string") {
    const exact = kindOf(raw);
    if (exact) return { kind: exact, message: "", raw };
    const head = raw.split(/[:(\s]/, 1)[0];
    const kind = kindOf(head);
    if (kind) return { kind, message: raw.slice(head.length).replace(/^[\s:(]+/, ""), raw };
    return { kind: "Other", message: raw, raw };
  }
  if (raw instanceof Error) {
    return { kind: "Other", message: raw.message, raw };
  }
  if (raw && typeof raw === "object") {
    const record = raw as Record<string, unknown>;
    const tagged = typeof record.kind === "string" ? record.kind : null;
    const message = [record.detail, record.message, record.error].find(
      (value): value is string => typeof value === "string",
    );
    if (tagged) return { kind: kindOf(tagged) ?? "Other", message: message ?? "", raw };
    const keys = Object.keys(record);
    if (keys.length === 1) {
      const kind = kindOf(keys[0]);
      const payload = record[keys[0]];
      if (kind) {
        if (typeof payload === "string") return { kind, message: payload, raw };
        return { kind, message: message ?? (payload == null ? "" : describe(payload)), raw };
      }
    }
    return { kind: "Other", message: message ?? describe(raw), raw };
  }
  return { kind: "Other", message: describe(raw), raw };
}

function describe(value: unknown): string {
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

export function isCdmError(value: unknown): value is CdmError {
  return (
    !!value &&
    typeof value === "object" &&
    typeof (value as CdmError).kind === "string" &&
    KINDS.indexOf((value as CdmError).kind) >= 0
  );
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    throw toCdmError(raw);
  }
}

export const listProfiles = () => call<ProfileStatus[]>("list_profiles");
export const createProfile = (name: string) => call<Profile>("create_profile", { name });
export const launchProfile = (id: string) => call<number>("launch_profile", { id });
export const renameProfile = (id: string, newName: string) =>
  call<Profile>("rename_profile", { id, newName });
export const deleteProfile = (id: string) => call<void>("delete_profile", { id });
export const quitProfile = (id: string) => call<void>("quit_profile", { id });
export const listAdoptable = () => call<AdoptCandidate[]>("list_adoptable");
export const adoptFolder = (dirName: string, displayName: string) =>
  call<Profile>("adopt_folder", { dirName, displayName });
export const openConfig = (id: string) => call<void>("open_config", { id });
export const doctor = () => call<DoctorReport>("doctor");
export const checkForUpdates = () => call<UpdateOutcome>("check_for_updates");
export const installUpdate = () => call<string>("install_update");
export const revealProfile = (id: string) => call<void>("reveal_profile", { id });

// Beyond the agreed command contract. Every flow below is specified by plan/03 but has no
// command yet; each caller degrades to the plan's error path when the backend rejects.
export const locateBinary = () => call<string | null>("locate_binary");
export const locateFolder = (id: string) => call<void>("locate_folder", { id });
export const removeFromList = (id: string) => call<void>("remove_from_list", { id });
export const forceQuitProfile = (id: string) => call<void>("force_quit_profile", { id });
export const deleteProfilePermanently = (id: string) =>
  call<void>("delete_profile_permanently", { id });
export const rebuildRegistry = () => call<ProfileStatus[]>("rebuild_registry");
export const revealRegistry = () => call<void>("reveal_registry");
export const openDownloadPage = () => call<void>("open_download_page");

export async function appVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "unknown";
  }
}

/** Closing is a request the Rust side intercepts to hide the window and drop the Dock icon. */
export function hideWindow(): void {
  try {
    void getCurrentWindow().close();
  } catch {
    /* not running inside Tauri */
  }
}

export function onTrayEvent(name: keyof typeof TRAY_EVENTS, handler: () => void): void {
  try {
    void listen(TRAY_EVENTS[name], () => handler());
  } catch {
    /* not running inside Tauri */
  }
}

export function onWindowShown(handler: () => void): void {
  try {
    void getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) handler();
    });
  } catch {
    /* not running inside Tauri */
  }
  window.addEventListener("focus", handler);
}
