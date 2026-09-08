import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { compile } from "tailwindcss";

const theme = await readFile(new URL("../src/styles/theme.generated.css", import.meta.url), "utf8");

test("semantic utilities, typography, dimensions and responsive variants compile", async () => {
  const compiler = await compile(`${theme}\n@tailwind utilities;`);
  const css = compiler.build([
    "bg-background-app",
    "text-text-body",
    "border-border-control",
    "shadow-modal",
    "text-page-title",
    "rounded-control",
    "p-8",
    "duration-hover",
    "ease-hover",
    "w-(--mona-layout-sidebar-width)",
    "shell:p-8",
    "@container",
    "@gallery-3:grid-cols-3",
    "bg-image-caption-background",
    "text-log-error",
  ]);
  for (const value of [
    "background-color: var(--mona-background-app)",
    "color: var(--mona-text-body)",
    "border-color: var(--mona-border-control)",
    "var(--mona-shadow-modal)",
    "font-size: var(--text-page-title)",
    "border-radius: var(--radius-control)",
    "padding: var(--spacing-8)",
    "transition-duration: var(--mona-motion-hover)",
    "width: var(--mona-layout-sidebar-width)",
    "width >= 900px",
    "width >= 43.25rem",
    "container-type: inline-size",
  ])
    assert.ok(css.includes(value), `Missing generated CSS: ${value}`);
});

test("default palette and off-scale spacing are not reintroduced", async () => {
  const compiler = await compile(`${theme}\n@tailwind utilities;`);
  const css = compiler.build(["bg-blue-500", "rounded-xl", "p-7", "shadow-lg"]);
  for (const selector of [".bg-blue-500", ".rounded-xl", ".p-7", ".shadow-lg"]) {
    assert.ok(!css.includes(selector), `Unexpected default token: ${selector}`);
  }
});
