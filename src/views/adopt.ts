import { adoptFolder, type AdoptCandidate, type CdmError } from "../api";
import { openDialog, textField } from "./dialog";
import { showNotice } from "./errors";
import { t } from "./strings";

export interface AdoptOptions {
  candidates: AdoptCandidate[];
  onAdopted: () => void;
}

export function renderAdoptBanner(
  count: number,
  onReview: () => void,
  onDismiss: () => void,
): HTMLElement {
  const banner = document.createElement("div");
  banner.className = "banner";

  const icon = document.createElement("span");
  icon.className = "banner-icon";
  icon.textContent = "ℹ";
  icon.setAttribute("aria-hidden", "true");

  const text = document.createElement("span");
  text.className = "banner-text";
  text.textContent = t.adopt.banner(count);

  const review = document.createElement("button");
  review.type = "button";
  review.className = "button";
  review.textContent = t.adopt.bannerAction;
  review.dataset.focusKey = "banner-review";
  review.addEventListener("click", onReview);

  const dismiss = document.createElement("button");
  dismiss.type = "button";
  dismiss.className = "banner-dismiss";
  dismiss.textContent = "✕";
  dismiss.setAttribute("aria-label", t.adopt.dismiss);
  dismiss.title = t.adopt.dismiss;
  dismiss.addEventListener("click", onDismiss);

  banner.append(icon, text, review, dismiss);
  return banner;
}

export function openAdoptSheet(options: AdoptOptions): void {
  const content = document.createElement("div");

  const body = document.createElement("p");
  body.className = "helper";
  body.textContent = t.adopt.body;
  content.append(body);

  const rows = options.candidates.map((candidate, index) => {
    const row = document.createElement("div");
    row.className = "adopt-row";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = true;
    checkbox.id = `adopt-${index}`;

    const folder = document.createElement("label");
    folder.className = "adopt-folder";
    folder.htmlFor = checkbox.id;
    folder.textContent = candidate.dirName;

    const name = textField({ label: t.adopt.nameLabel, value: candidate.suggestedName });

    row.append(checkbox, folder, name.row);
    content.append(row);
    checkbox.addEventListener("change", updateSubmit);
    return { candidate, checkbox, input: name.input };
  });

  const handle = openDialog({
    title: t.adopt.title,
    content,
    buttons: [
      { id: "cancel", label: t.common.cancel, role: "cancel" },
      { id: "add", label: t.adopt.submit(rows.length), role: "affirmative", onSelect: submit },
    ],
  });

  updateSubmit();

  function checked() {
    return rows.filter((row) => row.checkbox.checked);
  }

  function updateSubmit(): void {
    const count = checked().length;
    handle.setLabel("add", t.adopt.submit(Math.max(count, 1)));
    handle.setEnabled("add", count > 0);
  }

  async function submit(): Promise<void> {
    handle.setBusy(true);
    const failures: string[] = [];
    for (const row of checked()) {
      const name = row.input.value.trim() || row.candidate.suggestedName;
      try {
        await adoptFolder(row.candidate.dirName, name);
      } catch (error) {
        failures.push(`${row.candidate.dirName}: ${(error as CdmError).message}`);
      }
    }
    handle.close();
    options.onAdopted();
    if (failures.length > 0) showNotice(t.adopt.title, failures.join(" "));
  }
}
