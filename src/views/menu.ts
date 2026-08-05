export interface MenuItem {
  label: string;
  disabled?: boolean;
  destructive?: boolean;
  onSelect?: () => void;
}

export type MenuEntry = MenuItem | "separator";

let open: (() => void) | null = null;

export function closeMenu(): void {
  open?.();
}

export function openMenu(entries: MenuEntry[], at: { x: number; y: number } | HTMLElement): void {
  closeMenu();
  const layers = document.getElementById("layers") ?? document.body;
  const invoker = document.activeElement as HTMLElement | null;

  const menu = document.createElement("div");
  menu.className = "menu";
  menu.setAttribute("role", "menu");

  const items: HTMLButtonElement[] = [];
  for (const entry of entries) {
    if (entry === "separator") {
      const rule = document.createElement("div");
      rule.className = "menu-separator";
      rule.setAttribute("role", "separator");
      menu.append(rule);
      continue;
    }
    const item = document.createElement("button");
    item.type = "button";
    item.className = "menu-item";
    item.setAttribute("role", "menuitem");
    item.tabIndex = -1;
    item.textContent = entry.label;
    item.disabled = !!entry.disabled;
    if (entry.destructive) item.classList.add("is-destructive");
    item.addEventListener("click", () => {
      close();
      entry.onSelect?.();
    });
    items.push(item);
    menu.append(item);
  }

  menu.addEventListener("keydown", onKeydown);
  layers.append(menu);
  place(menu, at);
  items.find((item) => !item.disabled)?.focus();

  const dismiss = (event: MouseEvent) => {
    if (!menu.contains(event.target as Node)) close();
  };
  setTimeout(() => document.addEventListener("mousedown", dismiss), 0);
  open = close;

  function close(): void {
    document.removeEventListener("mousedown", dismiss);
    menu.remove();
    open = null;
    if (invoker && invoker.isConnected) invoker.focus();
  }

  function onKeydown(event: KeyboardEvent): void {
    const enabled = items.filter((item) => !item.disabled);
    const index = enabled.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      enabled[(index + 1) % enabled.length]?.focus();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      enabled[(index - 1 + enabled.length) % enabled.length]?.focus();
    } else if (event.key === "Home") {
      event.preventDefault();
      enabled[0]?.focus();
    } else if (event.key === "End") {
      event.preventDefault();
      enabled[enabled.length - 1]?.focus();
    } else if (event.key === "Tab") {
      event.preventDefault();
      close();
    }
  }
}

function place(menu: HTMLElement, at: { x: number; y: number } | HTMLElement): void {
  const point =
    at instanceof HTMLElement
      ? { x: at.getBoundingClientRect().left, y: at.getBoundingClientRect().bottom + 4 }
      : at;
  const rect = menu.getBoundingClientRect();
  const x = Math.max(4, Math.min(point.x, window.innerWidth - rect.width - 4));
  const y = Math.max(4, Math.min(point.y, window.innerHeight - rect.height - 4));
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
}
