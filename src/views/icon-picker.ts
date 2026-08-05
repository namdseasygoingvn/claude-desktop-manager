import type { GroupIcon } from "../api";
import { openDialog } from "./dialog";
import { icon, symbolNode } from "./icons";
import catalog from "./icon-catalog.json";
import { t } from "./strings";

export interface IconPickerOptions {
  current: GroupIcon | null;
  onSelect: (icon: GroupIcon | null) => void;
}

type Mode = "emoji" | "icons";

/** Notion-style icon chooser in a sheet: an Emoji | Icons toggle, a search field, a grid. */
export function openIconPicker(options: IconPickerOptions): void {
  const content = document.createElement("div");
  content.className = "icon-picker";

  const modeRow = document.createElement("div");
  modeRow.className = "picker-mode";

  const emojiMode = modeButton(t.groups.picker.emoji);
  const iconsMode = modeButton(t.groups.picker.icons);
  modeRow.append(emojiMode, iconsMode);

  const search = document.createElement("input");
  search.type = "search";
  search.className = "picker-search";
  search.placeholder = t.groups.picker.search;
  search.setAttribute("aria-label", t.groups.picker.search);
  search.autocomplete = "off";
  search.spellcheck = false;

  const grid = document.createElement("div");
  grid.className = "picker-grid";
  grid.setAttribute("role", "listbox");
  grid.setAttribute("aria-label", t.groups.picker.search);

  let mode: Mode = "emoji";
  let query = "";

  const handle = openDialog({
    title: t.groups.picker.title,
    content: build(),
    buttons: [
      {
        id: "remove",
        label: t.groups.picker.remove,
        role: "secondary",
        onSelect: () => {
          options.onSelect(null);
          handle.close();
        },
      },
      { id: "cancel", label: t.common.cancel, role: "cancel" },
    ],
  });

  handle.setEnabled("remove", options.current !== null);
  render();

  emojiMode.addEventListener("click", () => setMode("emoji"));
  iconsMode.addEventListener("click", () => setMode("icons"));
  search.addEventListener("input", () => {
    query = search.value;
    render();
  });
  search.focus();

  function build(): HTMLElement {
    content.append(modeRow, search, grid);
    return content;
  }

  function setMode(next: Mode): void {
    mode = next;
    emojiMode.classList.toggle("is-active", mode === "emoji");
    iconsMode.classList.toggle("is-active", mode === "icons");
    render();
  }

  function render(): void {
    grid.replaceChildren();
    const q = query.trim().toLowerCase();
    if (mode === "emoji") {
      for (const choice of catalog.emoji) {
        if (q && choice.char !== q && !choice.keywords.includes(q)) continue;
        grid.append(cell(choice));
      }
    } else {
      for (const choice of catalog.symbols) {
        if (q && !choice.name.toLowerCase().includes(q) && !choice.keywords.includes(q)) continue;
        grid.append(cell(choice));
      }
    }
  }

  function cell(
    choice: { char: string; keywords: string } | { name: string; keywords: string },
  ): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "picker-cell";
    button.setAttribute("role", "option");

    if (mode === "emoji") {
      const { char } = choice as { char: string };
      button.textContent = char;
      button.title = char;
      button.classList.toggle("is-selected", options.current?.emoji === char);
      button.addEventListener("click", () => pick({ emoji: char }));
    } else {
      const { name } = choice as { name: string };
      const svg = icon(symbolNode(name));
      svg.classList.add("picker-cell-icon");
      button.append(svg);
      button.title = name;
      button.classList.toggle("is-selected", options.current?.symbol === name);
      button.addEventListener("click", () => pick({ symbol: name }));
    }
    return button;
  }

  function pick(icon: GroupIcon): void {
    options.onSelect(icon);
    handle.close();
  }

  function modeButton(label: string): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "picker-mode-button";
    button.textContent = label;
    return button;
  }
}
