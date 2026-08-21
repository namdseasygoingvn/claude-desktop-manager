(function () {
  "use strict";

  if (window.__cdmRouteLock) return;
  window.__cdmRouteLock = true;

  var membersPath = window.__CDM_MEMBERS_PATH;
  if (typeof membersPath !== "string") return;

  var EXEMPT_PREFIXES = ["/login", "/magic-link", "/oauth", "/sso", "/verify"];

  function isExempt(path) {
    for (var i = 0; i < EXEMPT_PREFIXES.length; i++) {
      if (path.indexOf(EXEMPT_PREFIXES[i]) === 0) return true;
    }
    return false;
  }

  // Rolling 60s window of forced navigations, kept in sessionStorage: each location.replace
  // restarts this script, so an in-memory counter can't survive a forced navigation.
  function underCap() {
    try {
      var now = Date.now();
      var stamps = JSON.parse(sessionStorage.getItem("cdmRouteLockCap") || "[]");
      stamps = stamps.filter(function (stamp) {
        return now - stamp < 60000;
      });
      var ok = stamps.length < 5;
      if (ok) stamps.push(now);
      sessionStorage.setItem("cdmRouteLockCap", JSON.stringify(stamps));
      return ok;
    } catch (err) {
      return true;
    }
  }

  function enforce() {
    if (location.hostname !== "claude.ai") return;
    var path = location.pathname;
    if (path === membersPath || isExempt(path)) return;
    if (underCap()) location.replace(membersPath);
  }

  // A bare pushState/replaceState never repaints a React router, so the lock calls
  // through to the original first and only then checks the route it landed on.
  var originalPushState = history.pushState;
  history.pushState = function () {
    var result = originalPushState.apply(this, arguments);
    enforce();
    return result;
  };

  var originalReplaceState = history.replaceState;
  history.replaceState = function () {
    var result = originalReplaceState.apply(this, arguments);
    enforce();
    return result;
  };

  window.addEventListener("popstate", enforce);
  enforce();
})();
