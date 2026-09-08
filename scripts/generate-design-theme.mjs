import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { format } from "oxfmt";

const root = new URL("../", import.meta.url);
const tokens = JSON.parse(
  await readFile(new URL("design/MonaLauncher_Design_Tokens_v1.1.json", root), "utf8"),
);
const name = (value) =>
  value
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replaceAll(".", "-")
    .toLowerCase();
const rem = (value) => `${value / 16}rem`;
const declarations = (entries) => entries.map(([key, value]) => `  --${key}: ${value};`).join("\n");
const themed = (theme) =>
  declarations(
    Object.entries(tokens.color[theme]).map(([key, value]) => [`mona-${name(key)}`, value]),
  );
const variables = [];
function flatten(value, path) {
  for (const [key, item] of Object.entries(value)) {
    const next = `${path}-${name(key)}`;
    if (item !== null && typeof item === "object") flatten(item, next);
    else if (typeof item === "number") variables.push([next, rem(item)]);
    else if (typeof item === "string" && (item.includes(" / ") || item.endsWith("ch")))
      variables.push([next, item]);
  }
}
flatten(tokens.layout, "mona-layout");
flatten(tokens.controls, "mona-controls");
flatten(tokens.imageCaption, "mona-image-caption");
// Unitless counts and the viewport threshold must not be interpreted as lengths.
const raw = new Map(variables);
raw.set("mona-layout-gallery-max-columns", tokens.layout.galleryMaxColumns);
raw.set(
  "mona-layout-narrow-mode-viewport-width-below",
  `${tokens.layout.narrowMode.viewportWidthBelow}px`,
);
for (const [key, value] of Object.entries(tokens.motion)) {
  if (typeof value === "number")
    raw.set(
      `mona-motion-${name(key)}`,
      /Translation|TranslateY/.test(key) ? rem(value) : `${value}ms`,
    );
  else if (key.endsWith("Easing")) raw.set(`mona-motion-${name(key)}`, value);
}

const theme = [
  ["*", "initial"],
  ["color-transparent", "transparent"],
  ["color-current", "currentColor"],
  ["spacing-0", "0px"],
  ["spacing-px", "1px"],
  ...tokens.spacing.map((value) => [`spacing-${value / 4}`, rem(value)]),
  ...Object.entries(tokens.radius).map(([key, value]) => [`radius-${name(key)}`, rem(value)]),
  ["radius-tooltip", rem(tokens.controls.tooltipRadius)],
  ["radius-caption", rem(tokens.imageCaption.radius)],
  ["radius-full", "9999px"],
  ["font-sans", '"Geist Variable", "Noto Sans JP Variable", system-ui, sans-serif'],
  ["font-mono", '"Geist Mono Variable", "Noto Sans Mono CJK JP", ui-monospace, monospace'],
  ["font-weight-normal", 400],
  ["font-weight-medium", 500],
  ["font-weight-semibold", 600],
  ["breakpoint-shell", `${tokens.layout.narrowMode.viewportWidthBelow}px`],
  ["ease-hover", tokens.motion.hoverEasing],
  ["ease-modal", tokens.motion.modalEasing],
  ["aspect-gallery", tokens.layout.galleryAspectRatio],
];
for (const [key, value] of Object.entries(tokens.typography)) {
  if (Array.isArray(value)) continue;
  theme.push(
    [`text-${name(key)}`, rem(value.size)],
    [`text-${name(key)}--line-height`, String(value.lineHeight / value.size)],
    [`text-${name(key)}--font-weight`, value.weight],
  );
}
for (const [key, value] of Object.entries(tokens.layout.contentMaxWidth)) {
  if (value !== null) theme.push([`container-${name(key)}`, rem(value)]);
}
for (const [columns, value] of Object.entries(tokens.layout.galleryContainerMinWidthByColumns)) {
  if (value > 0) theme.push([`container-gallery-${columns}`, rem(value)]);
}
for (const [key, value] of Object.entries(tokens.log)) {
  if (typeof value === "string" && value.startsWith("#")) theme.push([`color-log-${key}`, value]);
}
theme.push(
  ["color-image-caption-background", tokens.imageCaption.background],
  ["color-image-caption-foreground", tokens.imageCaption.foreground],
);
const aliases = Object.keys(tokens.color.light).map((key) => [
  key.startsWith("shadow.") ? name(key) : `color-${name(key)}`,
  `var(--mona-${name(key)})`,
]);
let css = `/* Generated from design/MonaLauncher_Design_Tokens_v1.1.json. Run pnpm design:generate. */
@theme { ${declarations(theme)} }
@theme inline { ${declarations(aliases)} }
@layer base {
  :root { ${declarations([...raw])} ${themed("light")} color-scheme: light; }
  @media (prefers-color-scheme: dark) {
    :root:not([data-theme="light"]):not([data-theme="dark"]) { ${themed("dark")} color-scheme: dark; }
  }
  :root[data-theme="dark"] { ${themed("dark")} color-scheme: dark; }
}
`;
for (const key of ["hover", "menu", "modal", "groupCollapse"]) {
  css += `@utility duration-${name(key)} { transition-duration: var(--mona-motion-${name(key)}); }\n`;
}
const formatted = await format("theme.generated.css", css);
if (formatted.errors.length) throw new Error(JSON.stringify(formatted.errors));
css = formatted.code;
const output = new URL("src/styles/theme.generated.css", root);
if (process.argv.includes("--check")) {
  if ((await readFile(output, "utf8")) !== css)
    throw new Error("Design theme is stale. Run pnpm design:generate.");
} else {
  await writeFile(output, css);
  console.log(`Generated ${fileURLToPath(output)}`);
}
