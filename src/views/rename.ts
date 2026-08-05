import { renameProfile, type CdmError, type Profile, type ProfileStatus } from "../api";
import { openDialog, textField, type DialogHandle } from "./dialog";
import { showRenameFailed } from "./errors";
import { quitRunning } from "./quit";
import { t } from "./strings";

export interface RenameOptions {
  status: ProfileStatus;
  onRenamed: (profile: Profile) => void;
}

export function openRenameSheet(options: RenameOptions): void {
  const profile = options.status.profile;
  let running = options.status.runningPid !== null;

  const field = textField({ label: t.create.nameLabel, value: profile.name });

  const warning = document.createElement("p");
  warning.className = "warning";
  warning.setAttribute("role", "status");
  warning.textContent = t.rename.runningWarning(profile.name);
  warning.hidden = !running;

  const content = document.createElement("div");
  content.append(field.row, warning);

  const handle = openDialog({
    title: t.rename.title,
    content,
    buttons: [
      { id: "cancel", label: t.common.cancel, role: "cancel" },
      {
        id: "rename",
        label: running ? t.rename.submitRunning : t.rename.submit,
        role: "affirmative",
        onSelect: submit,
      },
    ],
  });

  field.input.addEventListener("input", () => {
    handle.setEnabled("rename", field.input.value.trim().length > 0);
  });

  async function submit(dialog: DialogHandle): Promise<void> {
    const newName = field.input.value.trim();
    if (!newName) return;

    if (running) {
      dialog.setBusy(true);
      dialog.setLabel("rename", t.rename.quitting);
      const down = await quitRunning(profile.id, profile.name);
      dialog.setLabel("rename", t.rename.submitRunning);
      dialog.setBusy(false);
      if (!down) return;
      running = false;
    }

    dialog.setBusy(true);
    try {
      const renamed = await renameProfile(profile.id, newName);
      dialog.close();
      options.onRenamed(renamed);
    } catch (error) {
      dialog.setBusy(false);
      const failure = error as CdmError;
      if (failure.kind === "ProfileRunning") {
        running = true;
        warning.hidden = false;
        dialog.setLabel("rename", t.rename.submitRunning);
        dialog.button("rename")?.focus();
        return;
      }
      if (failure.kind === "NameEmpty") return;
      showRenameFailed({
        operation: "rename",
        profile,
        onRetry: () => void submit(dialog),
      });
    }
  }
}
