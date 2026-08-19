import type { ProfileStatus } from "../api";
import { toggle } from "./general";
import { icon, Pencil } from "./icons";
import { runningLabel, syncLabel } from "./running";
import { nouns, t } from "./strings";
import { usageBars } from "./usage";

export interface DetailActions {
  launch: () => void;
  rename: () => void;
  reveal: () => void;
  remove: () => void;
  locate: () => void;
  removeFromList: () => void;
  editConfig: () => void;
  assignToGroup: () => void;
  /** null = membership unknown (status not loaded yet); the caller must hide the toggle, not disable it. */
  isSessionSyncMember: (id: string) => boolean | null;
  toggleSessionSync: (id: string) => void;
}

export interface DetailProps {
  status: ProfileStatus | null;
  launching: boolean;
  missing: boolean;
  showUsage: boolean;
  actions: DetailActions;
}

export function renderDetail(props: DetailProps): HTMLElement {
  const pane = document.createElement("section");
  pane.className = "detail";

  if (!props.status) return pane;

  const { profile } = props.status;
  const running = props.status.runningPid !== null;
  const syncMembership = props.actions.isSessionSyncMember(profile.id);

  const heading = document.createElement("h1");
  heading.className = "detail-name";
  heading.id = "detail-name";
  heading.textContent = profile.name;
  pane.setAttribute("aria-labelledby", heading.id);

  const secondLine = document.createElement("p");
  secondLine.className = "detail-status";
  secondLine.append(...statusLine(props, running));
  if (!props.missing && syncMembership === true) secondLine.append(syncLabel(t.sessionSync.statusLabel));

  const head = document.createElement("div");
  head.className = "detail-head";
  head.append(heading);
  if (!props.missing) head.append(renameButton(props.actions.rename));

  pane.append(head, secondLine);

  if (props.missing) {
    pane.append(orphanBody(props.actions));
    return pane;
  }

  const meters = props.showUsage && props.status.usage && usageBars(props.status.usage, "detail");
  if (meters) pane.append(meters);

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

  launchRow.append(launch);

  const rule = document.createElement("hr");

  const created = document.createElement("p");
  created.className = "detail-created";
  created.textContent = t.detail.created(profile.createdAt);

  const footer = document.createElement("div");
  footer.className = "detail-footer";

  if (!props.status.isDefaultInstall) {
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "button destructive";
    remove.textContent = t.detail.delete;
    remove.dataset.focusKey = "delete";
    remove.addEventListener("click", props.actions.remove);

    footer.append(remove);
  }

  pane.append(
    launchRow,
    actionList(profile.id, syncMembership, props.actions),
    rule,
    created,
    footer,
  );
  return pane;
}

function actionList(
  profileId: string,
  syncMembership: boolean | null,
  actions: DetailActions,
): HTMLElement {
  const list = document.createElement("div");
  list.className = "detail-action-list";

  list.append(
    actionRow(t.detail.editConfig, t.detail.editConfigHint, "edit-config", actions.editConfig),
    actionRow(nouns.revealItem, t.detail.revealHint, "reveal", actions.reveal),
    actionRow(t.groups.assignToGroup, t.detail.assignToGroupHint, "assign-group", actions.assignToGroup),
  );

  if (syncMembership !== null) {
    list.append(
      toggle({
        focusKey: "sync-sessions",
        label: t.sessionSync.label,
        hint: t.sessionSync.hint,
        checked: syncMembership,
        onChange: () => actions.toggleSessionSync(profileId),
      }),
    );
  }

  return list;
}

function actionRow(label: string, hint: string, focusKey: string, onClick: () => void): HTMLElement {
  const row = document.createElement("div");
  row.className = "detail-action";

  const hintLine = document.createElement("p");
  hintLine.className = "helper";
  hintLine.textContent = hint;

  row.append(action(label, focusKey, onClick), hintLine);
  return row;
}

function renameButton(onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "detail-rename";
  button.title = t.detail.rename;
  button.setAttribute("aria-label", t.detail.rename);
  button.dataset.focusKey = "rename";
  button.append(icon(Pencil));
  button.addEventListener("click", onClick);
  return button;
}

/** A live profile hands back two nodes: the animated word, then the plain timestamp after it. */
function statusLine(props: DetailProps, running: boolean): Node[] {
  if (props.missing) return [document.createTextNode(t.orphan.secondLine)];
  if (props.launching) return [runningLabel(t.detail.starting)];
  if (running) {
    const since = props.status?.profile.lastUsedAt;
    const label = runningLabel(t.detail.running);
    return since ? [label, document.createTextNode(t.detail.runningSince(since))] : [label];
  }
  const lastUsed = props.status?.profile.lastUsedAt;
  return [document.createTextNode(lastUsed ? t.detail.idle(lastUsed) : t.detail.neverLaunched)];
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
