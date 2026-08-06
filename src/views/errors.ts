import {
  appVersion,
  doctor,
  locateBinary,
  openDownloadPage,
  openReleasesPage,
  rebuildRegistry,
  revealRegistry,
  type CdmError,
  type Profile,
} from "../api";
import { openDialog, type DialogButton } from "./dialog";
import { t } from "./strings";

export type Operation =
  | "launch"
  | "create"
  | "rename"
  | "delete"
  | "quit"
  | "adopt"
  | "config"
  | "reveal";

export interface ErrorContext {
  operation: Operation;
  profile?: Profile;
  name?: string;
  onRetry?: () => void;
}

export function announce(text: string): void {
  const live = document.getElementById("live");
  if (live) live.textContent = text;
}

const cancelButton: DialogButton = { id: "cancel", label: t.common.cancel, role: "cancel" };

function okButton(onSelect?: () => void): DialogButton {
  return {
    id: "ok",
    label: t.common.ok,
    role: "affirmative",
    onSelect: (handle) => {
      handle.close();
      onSelect?.();
    },
  };
}

export function showNotice(message: string, informative?: string, onDismiss?: () => void): void {
  openDialog({ message, informative, buttons: [okButton(onDismiss)] });
}

export function showBinaryNotFound(onResolved?: () => void): void {
  const handle = openDialog({
    message: t.binary.message,
    informative: t.binary.informative,
    buttons: [
      {
        id: "locate",
        label: t.binary.locate,
        role: "affirmative",
        onSelect: () => pickBinary(),
      },
      {
        id: "get",
        label: t.binary.get,
        role: "secondary",
        onSelect: (dialog) => {
          void openDownloadPage().catch(() => undefined);
          dialog.close();
        },
      },
      cancelButton,
    ],
  });

  async function pickBinary(): Promise<void> {
    try {
      const picked = await locateBinary();
      if (!picked) return;
      handle.close();
      onResolved?.();
    } catch {
      showNotice(t.binary.wrongPickMessage, t.binary.wrongPickInformative, () => void pickBinary());
    }
  }
}

export function showTranslatedBuild(): void {
  openDialog({
    message: t.rosetta.message,
    informative: t.rosetta.informative,
    buttons: [
      {
        id: "download",
        label: t.rosetta.download,
        role: "affirmative",
        onSelect: (dialog) => {
          void openReleasesPage().catch(() => undefined);
          dialog.close();
        },
      },
      { id: "dismiss", label: t.rosetta.dismiss, role: "cancel" },
    ],
  });
}

function copyDetailsButton(error: CdmError, context: ErrorContext): DialogButton {
  return {
    id: "details",
    label: t.common.copyDetails,
    role: "secondary",
    onSelect: () => copyDetails(error, context),
  };
}

export function showLaunchFailed(error: CdmError, context: ErrorContext): void {
  const name = context.profile?.name ?? context.name ?? "";
  openDialog({
    message: t.launch.failedMessage(name),
    informative: sentence(error.message) || t.launch.failedFallback,
    buttons: [
      retryButton(t.common.tryAgain, context),
      copyDetailsButton(error, context),
      cancelButton,
    ],
  });
}

export function showRenameFailed(context: ErrorContext): void {
  const name = context.profile?.name ?? context.name ?? "";
  openDialog({
    message: t.rename.failedMessage(name),
    informative: t.rename.failedInformative,
    buttons: [retryButton(t.common.tryAgain, context), cancelButton],
  });
}

export function showCreateFailed(error: CdmError, name: string): void {
  openDialog({
    message: t.create.failedMessage(name),
    informative: sentence(error.message) || t.create.failedFallback,
    buttons: [okButton()],
  });
}

export function showDeleteFailed(context: ErrorContext, quitAndDelete?: () => void): void {
  const name = context.profile?.name ?? context.name ?? "";
  const primary: DialogButton = quitAndDelete
    ? {
        id: "quit-delete",
        label: t.remove.confirmRunning,
        role: "destructive",
        onSelect: (handle) => {
          handle.close();
          quitAndDelete();
        },
      }
    : retryButton(t.common.tryAgain, context);
  openDialog({
    message: t.remove.failedMessage(name),
    informative: t.remove.failedInformative,
    buttons: [primary, cancelButton],
  });
}

export function showPartialDelete(name: string): void {
  openDialog({
    message: t.remove.partialMessage(name),
    informative: t.remove.partialInformative,
    buttons: [okButton()],
  });
}

export function showQuitStuck(name: string, onForce: () => void, onCancel?: () => void): void {
  openDialog({
    message: t.quit.stuckMessage(name),
    informative: t.quit.stuckInformative,
    buttons: [
      {
        id: "force",
        label: t.quit.force,
        role: "destructive",
        onSelect: (handle) => {
          handle.close();
          onForce();
        },
      },
      {
        id: "cancel",
        label: t.common.cancel,
        role: "cancel",
        onSelect: (handle) => {
          handle.close();
          onCancel?.();
        },
      },
    ],
  });
}

/** Last-resort mapping. Flows that have a specific recovery handle the error before this. */
export function reportError(error: CdmError, context: ErrorContext): void {
  if (error.kind === "BinaryNotFound") {
    showBinaryNotFound(context.onRetry);
    return;
  }
  switch (context.operation) {
    case "launch":
      showLaunchFailed(error, context);
      return;
    case "create":
      showCreateFailed(error, context.name ?? "");
      return;
    case "rename":
      showRenameFailed(context);
      return;
    case "delete":
      showDeleteFailed(context);
      return;
    default:
      openDialog({
        message: t.common.couldntOpen(context.profile?.name ?? context.name ?? ""),
        informative: sentence(error.message),
        buttons: [copyDetailsButton(error, context), okButton()],
      });
  }
}

function retryButton(label: string, context: ErrorContext): DialogButton {
  return {
    id: "retry",
    label,
    role: "affirmative",
    onSelect: (handle) => {
      handle.close();
      context.onRetry?.();
    },
  };
}

function sentence(message: string): string {
  const trimmed = message.trim();
  if (!trimmed) return "";
  const match = /^[^.!?]*[.!?]/.exec(trimmed);
  const first = match ? match[0] : trimmed;
  return /[.!?]$/.test(first) ? first : `${first}.`;
}

async function copyDetails(error: CdmError, context: ErrorContext): Promise<void> {
  const report = await doctor().catch(() => undefined);
  const lines = [
    `cdm ${await appVersion()}`,
    navigator.userAgent,
    `Operation: ${context.operation}`,
    context.profile ? `Profile: ${context.profile.id} (${context.profile.dir})` : "",
    `Error: ${error.kind} ${error.message}`.trim(),
    report ? `Doctor: ${JSON.stringify(report)}` : "",
  ].filter(Boolean);
  await writeClipboard(lines.join("\n"));
  announce(t.common.copied);
}

export async function copyDiagnostics(): Promise<void> {
  const report = await doctor().catch((error: CdmError) => ({ error: error.message }));
  await writeClipboard([`cdm ${await appVersion()}`, navigator.userAgent, JSON.stringify(report)].join("\n"));
  announce(t.common.copied);
}

async function writeClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const scratch = document.createElement("textarea");
    scratch.value = text;
    scratch.setAttribute("aria-hidden", "true");
    scratch.className = "visually-hidden";
    document.body.append(scratch);
    scratch.select();
    document.execCommand("copy");
    scratch.remove();
  }
}

/**
 * §4.6 — a full-pane view rather than a dialog: there is nothing else the window can show.
 */
export function renderRegistryError(
  error: CdmError,
  onResolved: () => void,
  onRetry: () => void,
): HTMLElement {
  const corrupt = error.kind === "RegistryCorrupt";
  const pane = document.createElement("section");
  pane.className = "full-pane";

  const heading = document.createElement("h1");
  heading.textContent = corrupt ? t.registry.corruptHeading : t.registry.unreadableHeading;

  const body = document.createElement("p");
  body.textContent = corrupt
    ? t.registry.corruptBody
    : `${t.registry.unreadableBody} ${sentence(error.message)}`.trim();

  const actions = document.createElement("div");
  actions.className = "full-pane-actions";

  const primary = document.createElement("button");
  primary.type = "button";
  primary.className = "button primary";
  primary.dataset.focusKey = "registry-primary";
  primary.textContent = corrupt ? t.registry.rebuild : t.common.tryAgain;
  primary.addEventListener("click", () => {
    if (!corrupt) {
      onRetry();
      return;
    }
    void rebuildRegistry()
      .then(onResolved)
      .catch((failure: CdmError) => showNotice(t.registry.corruptHeading, sentence(failure.message)));
  });

  const secondary = document.createElement("button");
  secondary.type = "button";
  secondary.className = "button";
  secondary.textContent = corrupt ? t.registry.showDamaged : t.registry.showFile;
  secondary.addEventListener("click", () => void revealRegistry().catch(() => undefined));

  actions.append(primary, secondary);
  pane.append(heading, body);
  if (corrupt) {
    const rebuild = document.createElement("p");
    rebuild.textContent = t.registry.corruptRebuild;
    pane.append(rebuild);
  }
  pane.append(actions);
  return pane;
}
