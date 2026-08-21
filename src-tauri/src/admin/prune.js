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

  var HEADING_SELECTOR = "h1, h2, h3, h4, h5, h6, [role='heading']";
  var EXEMPT_SELECTOR = '[role="dialog"], [aria-modal="true"], [role="alert"], [role="status"]';

  var hideableBodyChildren = null;
  var applying = false;
  var rafScheduled = false;

  function findAnchor(main) {
    var headings = main.querySelectorAll(HEADING_SELECTOR);
    var match = null;
    var matchCount = 0;
    for (var i = 0; i < headings.length; i++) {
      if (/members/i.test(headings[i].textContent || "")) {
        matchCount++;
        match = headings[i];
      }
    }
    if (matchCount !== 1) return main;

    var node = match.parentElement;
    while (node && node !== main) {
      if (node.children.length > 1) return node;
      node = node.parentElement;
    }
    return main;
  }

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
  }

  function applyPrune() {
    var main = document.querySelector("main");
    if (!main) return;

    if (hideableBodyChildren === null) snapshotBodyChildren();

    var chain = buildChain(findAnchor(main));
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
  }

  function evaluate() {
    if (onMembersRoute()) {
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

  window.addEventListener("popstate", scheduleApply);
  scheduleApply();
})();
