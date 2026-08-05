import { t } from "./strings";

export interface EmptyProps {
  adoptable: boolean;
  onNew: () => void;
  onAdopt: () => void;
}

/** §3.1 — first run is a normal state with exactly one obvious next action. */
export function renderEmpty(props: EmptyProps): HTMLElement {
  const pane = document.createElement("section");
  pane.className = "full-pane empty";

  const heading = document.createElement("h1");
  heading.textContent = t.empty.heading;

  const body = document.createElement("p");
  body.textContent = t.empty.body;

  const primary = document.createElement("button");
  primary.type = "button";
  primary.className = "button primary large";
  primary.textContent = t.empty.primary;
  primary.dataset.focusKey = "new-profile";
  primary.addEventListener("click", props.onNew);

  pane.append(heading, body, primary);

  if (props.adoptable) {
    const link = document.createElement("button");
    link.type = "button";
    link.className = "link";
    link.textContent = t.empty.adoptLink;
    link.addEventListener("click", props.onAdopt);
    pane.append(link);
  }

  return pane;
}
