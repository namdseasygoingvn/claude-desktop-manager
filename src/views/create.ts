import { createProfile, type CdmError, type Profile } from "../api";
import { openDialog, textField } from "./dialog";
import { showCreateFailed } from "./errors";
import { t } from "./strings";

export interface CreateOptions {
  existingNames: string[];
  onCreated: (profile: Profile) => void;
}

export function openCreateSheet(options: CreateOptions): void {
  const field = textField({ label: t.create.nameLabel, placeholder: t.create.placeholder });

  const helper = document.createElement("p");
  helper.className = "helper";
  helper.id = `${field.input.id}-helper`;
  helper.textContent = t.create.helper;
  field.input.setAttribute("aria-describedby", helper.id);

  const duplicate = document.createElement("p");
  duplicate.className = "note";
  duplicate.hidden = true;
  duplicate.setAttribute("role", "status");

  const content = document.createElement("div");
  content.append(field.row, helper, duplicate);

  const handle = openDialog({
    title: t.create.title,
    content,
    buttons: [
      { id: "cancel", label: t.common.cancel, role: "cancel" },
      { id: "create", label: t.create.submit, role: "affirmative", onSelect: submit },
    ],
  });

  handle.setEnabled("create", false);
  field.input.addEventListener("input", validate);

  function validate(): void {
    const name = field.input.value.trim();
    handle.setEnabled("create", name.length > 0);
    const clash = options.existingNames.some((existing) => existing === name);
    duplicate.hidden = !clash;
    duplicate.textContent = clash ? t.create.duplicate(name) : "";
  }

  async function submit(): Promise<void> {
    const name = field.input.value.trim();
    if (!name) return;
    handle.setBusy(true);
    try {
      const profile = await createProfile(name);
      handle.close();
      options.onCreated(profile);
    } catch (error) {
      handle.setBusy(false);
      validate();
      const failure = error as CdmError;
      if (failure.kind !== "NameEmpty") showCreateFailed(failure, name);
      field.input.focus();
      field.input.select();
    }
  }
}
