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

  var EXEMPT_SELECTOR = '[role="dialog"], [aria-modal="true"], [role="alert"], [role="status"]';

  var hideableBodyChildren = null;
  var applying = false;
  var rafScheduled = false;
  var enabled = true;

  function buildChain(anchor) {
    var chain = [];
    var node = anchor;
    while (node && node.parentElement) {
      chain.push(node);
      if (node.parentElement === document.body) break;
      node = node.parentElement;
    }
    return chain;
  }

  function isExempt(el) {
    return !!(el.matches && el.matches(EXEMPT_SELECTOR));
  }

  function isNonEmpty(el) {
    if (el.children && el.children.length > 0) return true;
    return /\S/.test(el.textContent || "");
  }

  // Snapshot taken once, at the first successful pass: a body-level container that's empty
  // here may still be a portal host claude.ai mounts a dialog into later, so only containers
  // already carrying real content at snapshot time are ever eligible to hide.
  function snapshotBodyChildren() {
    hideableBodyChildren = new WeakSet();
    var children = document.body.children;
    for (var i = 0; i < children.length; i++) {
      var child = children[i];
      if (isExempt(child)) continue;
      if (isNonEmpty(child)) hideableBodyChildren.add(child);
    }
  }

  function hide(el) {
    if (el.hasAttribute("data-cdm-hidden")) return;
    el.style.setProperty("display", "none", "important");
    el.setAttribute("data-cdm-hidden", "1");
  }

  // Inverse of hide(): run whenever the route leaves members, so a login form or nav that got
  // hidden while on the members route never stays hidden after navigating away.
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

  function applyPrune() {
    var main = document.querySelector("main");
    if (!main) return;

    if (hideableBodyChildren === null) snapshotBodyChildren();

    var chain = buildChain(main);
    for (var i = 0; i < chain.length; i++) {
      var node = chain[i];
      var parent = node.parentElement;
      if (!parent) continue;
      var atBody = parent === document.body;
      var siblings = parent.children;
      for (var j = 0; j < siblings.length; j++) {
        var sib = siblings[j];
        if (sib === node) continue;
        if (isExempt(sib)) continue;
        if (atBody && !hideableBodyChildren.has(sib)) continue;
        hide(sib);
      }
    }

    pruneInsideMain(main);
  }

  // Anchor for narrowing below <main>: the outermost <section> wrapping the members table.
  // A heading-derived anchor once hid the table itself, and anchoring on the table would
  // orphan its own title/filter/pagination rows — the section is the smallest container
  // holding them all. No table or no section yet -> null, and applyPrune leaves the inside
  // of main alone (fail open).
  function memberSectionAnchor(main) {
    var table = main.querySelector("table");
    if (!table) return null;
    var section = null;
    var node = table.parentElement;
    while (node && node !== main) {
      if (node.tagName === "SECTION") section = node;
      node = node.parentElement;
    }
    return section;
  }

  // The settings layout is a grid reserving a fixed sidebar column; with the nav hidden the
  // content reflows into that narrow column, so the template must collapse alongside it.
  function collapseGrid(parent) {
    if (parent.hasAttribute("data-cdm-degrid")) return;
    if (getComputedStyle(parent).display !== "grid") return;
    parent.style.setProperty("grid-template-columns", "minmax(0px, 1fr)", "important");
    parent.setAttribute("data-cdm-degrid", "1");
  }

  function pruneInsideMain(main) {
    var anchor = memberSectionAnchor(main);
    if (!anchor) return;
    var node = anchor;
    while (node && node !== main) {
      var parent = node.parentElement;
      if (!parent) return;
      var siblings = parent.children;
      var anyHidden = false;
      for (var j = 0; j < siblings.length; j++) {
        var sib = siblings[j];
        if (sib === node) continue;
        if (isExempt(sib)) continue;
        // No snapshot below main (React re-creates this subtree too often for element
        // identity to hold), so the portal-host defense here is: never hide an empty
        // container, nor one already hosting exempt UI.
        if (!isNonEmpty(sib)) continue;
        if (sib.querySelector(EXEMPT_SELECTOR)) continue;
        hide(sib);
        anyHidden = true;
      }
      if (anyHidden) collapseGrid(parent);
      node = parent;
    }
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

  // Doubles as the initial waiter: main doesn't exist at document start, so the first
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
