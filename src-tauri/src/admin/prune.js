(function () {
  "use strict";

  if (window.__cdmPrune) return;
  window.__cdmPrune = true;

  var membersPath = window.__CDM_MEMBERS_PATH;
  if (typeof membersPath !== "string") return;

  // Re-checked on every pass, not just at script load: claude.ai is a SPA, so the route can
  // change (e.g. login -> members, or members -> some other admin page) without a fresh
  // document and a fresh run of this script.
  function onMembersRoute() {
    return location.hostname === "claude.ai" && location.pathname === membersPath;
  }

  var enabled = true;
  var applying = false;
  var rafScheduled = false;

  function hide(el) {
    if (el.hasAttribute("data-cdm-hidden")) return;
    el.style.setProperty("display", "none", "important");
    el.setAttribute("data-cdm-hidden", "1");
  }

  // The settings layout is a grid reserving a fixed column for the sidebar; with the nav hidden
  // the content would reflow into that narrow column, so the template collapses alongside it.
  function collapseGrid(parent) {
    if (parent.hasAttribute("data-cdm-degrid")) return;
    if (getComputedStyle(parent).display !== "grid") return;
    parent.style.setProperty("grid-template-columns", "minmax(0px, 1fr)", "important");
    parent.setAttribute("data-cdm-degrid", "1");
  }

  // Inverse of hide() and collapseGrid(): runs whenever the route leaves members or the toggle
  // goes off, so nothing stays hidden while this script is not meant to be in charge.
  function restore() {
    var hidden = document.querySelectorAll("[data-cdm-hidden]");
    for (var i = 0; i < hidden.length; i++) {
      hidden[i].style.removeProperty("display");
      hidden[i].removeAttribute("data-cdm-hidden");
    }
    var degridded = document.querySelectorAll("[data-cdm-degrid]");
    for (var j = 0; j < degridded.length; j++) {
      degridded[j].style.removeProperty("grid-template-columns");
      degridded[j].removeAttribute("data-cdm-degrid");
    }
  }

  // The settings sidebar, and nothing else. Every broader rule tried here — body-level siblings,
  // ancestor-chain walks, a section anchor — had to model claude.ai's layout, and hid the members
  // table itself as soon as that layout moved. No nav yet: nothing happens, and the next
  // observer pass tries again.
  function applyPrune() {
    var nav = document.querySelector("main nav");
    if (!nav) return;
    hide(nav);
    if (nav.parentElement) collapseGrid(nav.parentElement);
  }

  function evaluate() {
    if (enabled && onMembersRoute()) {
      applyPrune();
    } else {
      restore();
    }
  }

  function scheduleApply() {
    if (rafScheduled) return;
    rafScheduled = true;
    requestAnimationFrame(function () {
      rafScheduled = false;
      applying = true;
      evaluate();
      // Deferred to a microtask so it runs after any MutationObserver callback queued by this
      // pass's own hides — reset synchronously instead and that callback would still see
      // applying === false, scheduling an immediate, unnecessary re-pass.
      Promise.resolve().then(function () {
        applying = false;
      });
    });
  }

  // Doubles as the initial waiter: the nav doesn't exist at document start, so the first
  // successful pass runs once React mounts it and this observer sees the insertion.
  var observer = new MutationObserver(function () {
    if (!applying) scheduleApply();
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });

  // claude.ai's router changes location via pushState/replaceState, which fires neither
  // popstate nor (necessarily) a DOM mutation this observer would catch synchronously.
  // Formerly route_lock.js's hooks; kept here, without any forced navigation, purely so
  // evaluate() re-runs on every route change.
  var originalPushState = history.pushState;
  history.pushState = function () {
    var result = originalPushState.apply(this, arguments);
    scheduleApply();
    return result;
  };

  var originalReplaceState = history.replaceState;
  history.replaceState = function () {
    var result = originalReplaceState.apply(this, arguments);
    scheduleApply();
    return result;
  };

  // Global on purpose: admin::toggle_prune evals this when Cmd/Ctrl+H lands in the main
  // window (tab strip focused) instead of in this webview.
  window.__cdmPruneToggle = function () {
    enabled = !enabled;
    scheduleApply();
  };

  // Capture + preventDefault so the shortcut beats both claude.ai's own handlers and, on
  // macOS, the default app menu's ⌘H Hide item.
  window.addEventListener(
    "keydown",
    function (event) {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.shiftKey || event.altKey) return;
      if ((event.key || "").toLowerCase() !== "h") return;
      event.preventDefault();
      event.stopPropagation();
      window.__cdmPruneToggle();
    },
    true
  );

  window.addEventListener("popstate", scheduleApply);
  scheduleApply();
})();
