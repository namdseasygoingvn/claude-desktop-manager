import type { Theme } from "../api";

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

let chosen: Theme = "system";

/** Paint the document, then keep following the system for as long as that is the choice. */
export function applyTheme(theme: Theme): void {
  chosen = theme;
  document.documentElement.dataset.theme = resolve(theme);
}

function resolve(theme: Theme): "light" | "dark" {
  if (theme !== "system") return theme;
  return systemDark.matches ? "dark" : "light";
}

systemDark.addEventListener("change", () => applyTheme(chosen));
