import type { ProfileStatus } from "../api";
import { FolderOpen, icon } from "./icons";
import { t } from "./strings";

export interface DetailActions {
  launch: () => void;
  rename: () => void;
  reveal: () => void;
  remove: () => void;
  locate: () => void;
  removeFromList: () => void;
}

export interface DetailProps {
  status: ProfileStatus | null;
  launching: boolean;
  missing: boolean;
  actions: DetailActions;
}

export function renderDetail(props: DetailProps): HTMLElement {
  const pane = document.createElement("section");
  pane.className = "detail";

  if (!props.status) return pane;

  const { profile } = props.status;
  const running = props.status.runningPid !== null;

  const heading = document.createElement("h1");
  heading.className = "detail-name";
  heading.id = "detail-name";
  heading.textContent = profile.name;
  pane.setAttribute("aria-labelledby", heading.id);

  const secondLine = document.createElement("p");
  secondLine.className = "detail-status";
  secondLine.textContent = statusLine(props, running);

  pane.append(heading, secondLine);

  if (props.missing) {
    pane.append(orphanBody(props.actions));
    return pane;
  }

  if (!profile.lastUsedAt && !props.launching) {
    const hint = document.createElement("p");
    hint.className = "helper";
    hint.textContent = t.detail.neverLaunchedHint;
    pane.append(hint);
  }

  const launchRow = document.createElement("div");
  launchRow.className = "detail-launch";

  const launch = document.createElement("button");
  launch.type = "button";
  launch.className = "button primary large";
  launch.textContent = props.launching ? t.detail.launching : t.detail.launch;
  launch.disabled = props.launching;
  launch.dataset.focusKey = "primary";
  launch.addEventListener("click", props.actions.launch);

  const reveal = document.createElement("button");
  reveal.type = "button";
  reveal.className = "button detail-reveal";
  reveal.title = t.detail.reveal;
  reveal.setAttribute("aria-label", t.detail.reveal);
  reveal.append(icon(FolderOpen));
  reveal.addEventListener("click", props.actions.reveal);

  launchRow.append(launch, reveal);

  const row = document.createElement("div");
  row.className = "detail-actions";
  row.append(action(t.detail.rename, "rename", props.actions.rename));

  const rule = document.createElement("hr");

  const created = document.createElement("p");
  created.className = "detail-created";
  created.textContent = t.detail.created(profile.createdAt);

  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "link destructive";
  remove.textContent = t.detail.delete;
  remove.dataset.focusKey = "delete";
  remove.addEventListener("click", props.actions.remove);

  pane.append(launch, row, rule, created, remove);
  return pane;
}

function statusLine(props: DetailProps, running: boolean): string {
  if (props.missing) return t.orphan.secondLine;
  if (props.launching) return t.detail.starting;
  if (running) return t.detail.running(props.status?.profile.lastUsedAt ?? null);
  const lastUsed = props.status?.profile.lastUsedAt;
  return lastUsed ? t.detail.idle(lastUsed) : t.detail.neverLaunched;
}

function orphanBody(actions: DetailActions): HTMLElement {
  const wrap = document.createElement("div");

  const body = document.createElement("p");
  body.className = "helper";
  body.textContent = t.orphan.body;

  const row = document.createElement("div");
  row.className = "detail-actions";

  const locate = document.createElement("button");
  locate.type = "button";
  locate.className = "button primary";
  locate.textContent = t.orphan.locate;
  locate.dataset.focusKey = "primary";
  locate.addEventListener("click", actions.locate);

  row.append(locate, action(t.orphan.remove, "remove-from-list", actions.removeFromList));
  wrap.append(body, row);
  return wrap;
}

function action(label: string, focusKey: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "button";
  button.textContent = label;
  button.dataset.focusKey = focusKey;
  button.addEventListener("click", onClick);
  return button;
}
