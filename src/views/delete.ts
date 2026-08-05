import {
  deleteProfile,
  deleteProfilePermanently,
  removeFromList,
  type CdmError,
  type Profile,
  type ProfileStatus,
} from "../api";
import { openDialog, type DialogHandle } from "./dialog";
import { showDeleteFailed, showNotice } from "./errors";
import { quitRunning } from "./quit";
import { t } from "./strings";

export interface DeleteOptions {
  status: ProfileStatus;
  onDeleted: () => void;
}

export function confirmDelete(options: DeleteOptions): void {
  const profile = options.status.profile;
  let running = options.status.runningPid !== null;

  openDialog({
    message: running ? t.remove.runningMessage(profile.name) : t.remove.message(profile.name),
    informative: running ? t.remove.runningInformative(profile.name) : t.remove.informative,
    buttons: [
      {
        id: "delete",
        label: running ? t.remove.confirmRunning : t.remove.confirm,
        role: "destructive",
        onSelect: confirm,
      },
      { id: "cancel", label: t.common.cancel, role: "cancel" },
    ],
  });

  async function confirm(dialog: DialogHandle): Promise<void> {
    if (running) {
      dialog.setBusy(true);
      const down = await quitRunning(profile.id, profile.name);
      dialog.setBusy(false);
      if (!down) return;
      running = false;
    }

    dialog.setBusy(true);
    try {
      await deleteProfile(profile.id);
      dialog.close();
      options.onDeleted();
    } catch (error) {
      dialog.setBusy(false);
      const failure = error as CdmError;
      if (failure.kind === "ProfileRunning") {
        running = true;
        showRunningConsequence(dialog, profile);
        return;
      }
      dialog.close();
      if (/trash|recycle/i.test(failure.message)) {
        confirmPermanentDelete(options);
        return;
      }
      showDeleteFailed(
        { operation: "delete", profile, onRetry: () => confirmDelete(options) },
        running ? () => confirmDelete(options) : undefined,
      );
    }
  }
}

function showRunningConsequence(dialog: DialogHandle, profile: Profile): void {
  dialog.setMessage(t.remove.runningMessage(profile.name));
  dialog.setInformative(t.remove.runningInformative(profile.name));
  dialog.setLabel("delete", t.remove.confirmRunning);
  dialog.button("cancel")?.focus();
}

/** §3.5 — permanent deletion is never taken silently; it is always its own decision. */
function confirmPermanentDelete(options: DeleteOptions): void {
  const profile = options.status.profile;
  openDialog({
    message: t.remove.trashFailedMessage(profile.name),
    informative: t.remove.trashFailedInformative,
    buttons: [
      {
        id: "permanent",
        label: t.remove.deletePermanently,
        role: "destructive",
        onSelect: async (dialog) => {
          dialog.setBusy(true);
          try {
            await deleteProfilePermanently(profile.id);
            dialog.close();
            options.onDeleted();
          } catch {
            dialog.close();
            showDeleteFailed({ operation: "delete", profile, onRetry: () => confirmDelete(options) });
          }
        },
      },
      { id: "cancel", label: t.common.cancel, role: "cancel" },
    ],
  });
}

export interface RemoveOptions {
  profile: Profile;
  onRemoved: () => void;
}

/** §3.7 — removing an orphan from the list, which deletes nothing. */
export function confirmRemoveFromList(options: RemoveOptions): void {
  openDialog({
    message: t.orphan.removeMessage(options.profile.name),
    informative: t.orphan.removeInformative,
    buttons: [
      {
        id: "remove",
        label: t.orphan.removeConfirm,
        role: "destructive",
        onSelect: async (dialog) => {
          dialog.setBusy(true);
          try {
            await removeFromList(options.profile.id);
            dialog.close();
            options.onRemoved();
          } catch (error) {
            dialog.close();
            showNotice(t.orphan.removeMessage(options.profile.name), (error as CdmError).message);
          }
        },
      },
      { id: "cancel", label: t.common.cancel, role: "cancel" },
    ],
  });
}
