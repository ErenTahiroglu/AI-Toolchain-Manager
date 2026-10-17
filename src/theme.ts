export type ThemeChoice = "system" | "light" | "dark";

export function isThemeChoice(v: string | undefined): v is ThemeChoice {
  return v === "system" || v === "light" || v === "dark";
}

/** `system` removes the override so the CSS `prefers-color-scheme` rule decides. */
export function applyTheme(choice: ThemeChoice, root: HTMLElement = document.documentElement): void {
  if (choice === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", choice);
}
