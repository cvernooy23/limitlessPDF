/**
 * Theme controller. Three preferences — "system" (follow the OS), "light",
 * "dark" — persisted in localStorage. The *resolved* value ("light" | "dark")
 * is mirrored onto <html data-theme> (also pre-applied by an inline script in
 * index.html to avoid a first-paint flash). When the preference is "system",
 * the resolved value tracks the OS setting live.
 */
export type ThemePref = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "lp-theme";

function systemResolved(): ResolvedTheme {
  if (typeof window !== "undefined" && window.matchMedia) {
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  return "dark";
}

function loadPref(): ThemePref {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* localStorage may be unavailable; fall back to system. */
  }
  return "system";
}

function applyResolved(resolved: ResolvedTheme) {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", resolved);
  }
}

class ThemeController {
  /** The user's stored preference. */
  pref = $state<ThemePref>(loadPref());
  /** The theme actually in effect right now. */
  resolved = $state<ResolvedTheme>("dark");

  constructor() {
    this.resolved = this.pref === "system" ? systemResolved() : this.pref;
    applyResolved(this.resolved);

    if (typeof window !== "undefined" && window.matchMedia) {
      window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => {
        if (this.pref === "system") {
          this.resolved = systemResolved();
          applyResolved(this.resolved);
        }
      });
    }
  }

  /** Set an explicit preference (or "system" to follow the OS). */
  set(pref: ThemePref) {
    this.pref = pref;
    try {
      localStorage.setItem(STORAGE_KEY, pref);
    } catch {
      /* best-effort persistence */
    }
    this.resolved = pref === "system" ? systemResolved() : pref;
    applyResolved(this.resolved);
  }

  /** Flip to the opposite of what's currently shown (an explicit choice). */
  toggle() {
    this.set(this.resolved === "dark" ? "light" : "dark");
  }
}

export const theme = new ThemeController();
