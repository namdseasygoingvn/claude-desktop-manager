import { createElement, type IconNode } from "lucide";

export { ChevronDown, Ellipsis, Info, Minus, Plus, X, type IconNode } from "lucide";

/** Every icon is decorative: the control it sits in carries the accessible name. */
export function icon(node: IconNode): SVGElement {
  const svg = createElement(node);
  svg.classList.add("icon");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  return svg;
}
