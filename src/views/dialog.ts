import { defaultButton, orderButtons, type ButtonRole } from "./platform";

export interface DialogButton {
  id: string;
  label: string;
  role: ButtonRole;
  onSelect?: (handle: DialogHandle) => void | Promise<void>;
}

export interface DialogSpec {
  /** Sheet title, for dialogs that collect input. */
  title?: string;
  /** Alert message text — bold, short, ends in "?" for confirmations. */
  message?: string;
  informative?: string;
  content?: HTMLElement;
  buttons: DialogButton[];
}

export interface DialogHandle {
  root: HTMLElement;
  close(): void;
  button(id: string): HTMLButtonElement | undefined;
  setEnabled(id: string, enabled: boolean): void;
  setLabel(id: string, label: string): void;
  setBusy(busy: boolean): void;
  setMessage(text: string): void;
  setInformative(text: string): void;
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

let sequence = 0;
const stack: DialogHandle[] = [];

export function isDialogOpen(): boolean {
  return stack.length > 0;
}

export function openDialog(spec: DialogSpec): DialogHandle {
  const layers = document.getElementById("layers") ?? document.body;
  const invoker = document.activeElement as HTMLElement | null;
  const uid = `dlg-${++sequence}`;

  const scrim = document.createElement("div");
  scrim.className = "scrim";

  const root = document.createElement("div");
  root.className = "dialog";
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-modal", "true");

  if (spec.title) {
    const title = element("h2", "dialog-title", spec.title, `${uid}-title`);
    root.append(title);
    root.setAttribute("aria-labelledby", title.id);
  }
  let messageNode: HTMLElement | undefined;
  if (spec.message) {
    messageNode = element("p", "dialog-message", spec.message, `${uid}-message`);
    root.append(messageNode);
    root.setAttribute("aria-labelledby", messageNode.id);
  }
  if (spec.content) {
    const body = document.createElement("div");
    body.className = "dialog-content";
    body.append(spec.content);
    root.append(body);
  }
  let informativeNode: HTMLElement | undefined;
  if (spec.informative) {
    informativeNode = element("p", "dialog-informative", spec.informative, `${uid}-info`);
    root.append(informativeNode);
    root.setAttribute("aria-describedby", informativeNode.id);
  }

  const footer = document.createElement("div");
  footer.className = "dialog-footer";
  root.append(footer);

  const buttons = new Map<string, HTMLButtonElement>();
  const preferred = defaultButton(spec.buttons);
  const cancel = spec.buttons.find((b) => b.role === "cancel");

  const handle: DialogHandle = {
    root,
    close,
    button: (id) => buttons.get(id),
    setEnabled(id, enabled) {
      const button = buttons.get(id);
      if (button) button.disabled = !enabled;
    },
    setLabel(id, label) {
      const button = buttons.get(id);
      if (button) button.textContent = label;
    },
    setBusy(busy) {
      root.classList.toggle("is-busy", busy);
      if (busy) {
        alreadyDisabled.clear();
        for (const [id, button] of buttons) {
          if (button.disabled) alreadyDisabled.add(id);
          button.disabled = true;
        }
      } else {
        for (const [id, button] of buttons) button.disabled = alreadyDisabled.has(id);
      }
    },
    setMessage(text) {
      if (messageNode) messageNode.textContent = text;
    },
    setInformative(text) {
      if (informativeNode) informativeNode.textContent = text;
    },
  };

  const alreadyDisabled = new Set<string>();

  for (const spec_ of orderButtons(spec.buttons)) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = spec_.label;
    button.className = `dialog-button role-${spec_.role}`;
    if (preferred && spec_.id === preferred.id) button.classList.add("is-default");
    button.addEventListener("click", () => {
      void activate(spec_);
    });
    buttons.set(spec_.id, button);
    footer.append(button);
  }

  root.addEventListener("keydown", onKeydown);
  scrim.append(root);
  layers.append(scrim);
  stack.push(handle);

  const field = root.querySelector<HTMLInputElement>("input[type=text]");
  if (field) {
    field.focus();
    field.select();
  } else if (preferred) {
    buttons.get(preferred.id)?.focus();
  } else {
    root.querySelector<HTMLElement>(FOCUSABLE)?.focus();
  }

  return handle;

  async function activate(button: DialogButton): Promise<void> {
    if (button.onSelect) await button.onSelect(handle);
    else if (button.role === "cancel") close();
  }

  function onKeydown(event: KeyboardEvent): void {
    if (stack[stack.length - 1] !== handle) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (cancel) void activate(cancel);
      else close();
      return;
    }
    if (event.key === "Enter") {
      const target = event.target as HTMLElement;
      if (target.tagName === "BUTTON" || target.tagName === "TEXTAREA") return;
      event.preventDefault();
      event.stopPropagation();
      if (preferred && !buttons.get(preferred.id)?.disabled) void activate(preferred);
      return;
    }
    if (event.key === "Tab") trapFocus(event, root);
  }

  function close(): void {
    root.removeEventListener("keydown", onKeydown);
    scrim.remove();
    const index = stack.indexOf(handle);
    if (index >= 0) stack.splice(index, 1);
    restoreFocus();
  }

  /** The invoking control may have been replaced by a re-render while the dialog was open. */
  function restoreFocus(): void {
    if (invoker?.isConnected) {
      invoker.focus();
      return;
    }
    const key = invoker?.dataset?.focusKey;
    if (key) document.querySelector<HTMLElement>(`[data-focus-key="${key}"]`)?.focus();
  }
}

export interface TextField {
  row: HTMLElement;
  input: HTMLInputElement;
}

export function textField(spec: {
  label: string;
  value?: string;
  placeholder?: string;
}): TextField {
  const row = document.createElement("div");
  row.className = "field";

  const input = document.createElement("input");
  input.type = "text";
  input.id = `field-${++sequence}`;
  input.value = spec.value ?? "";
  if (spec.placeholder) input.placeholder = spec.placeholder;
  input.autocomplete = "off";
  input.spellcheck = false;

  const label = document.createElement("label");
  label.textContent = spec.label;
  label.htmlFor = input.id;

  row.append(label, input);
  return { row, input };
}

function element(tag: string, className: string, text: string, id: string): HTMLElement {
  const node = document.createElement(tag);
  node.className = className;
  node.textContent = text;
  node.id = id;
  return node;
}

function trapFocus(event: KeyboardEvent, root: HTMLElement): void {
  const focusable = Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (node) => node.offsetParent !== null || node === document.activeElement,
  );
  if (focusable.length === 0) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
