export function readTheme() {
  try {
    const value = localStorage.getItem("mona:theme");
    return value === "light" || value === "dark" ? value : "system";
  } catch {
    return "system";
  }
}

export function applyStoredTheme() {
  document.documentElement.dataset.theme = readTheme();
}
