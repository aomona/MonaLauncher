import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Feed } from "feed";
import matter from "gray-matter";
import { Marked } from "marked";
import { parse as parseYaml } from "yaml";
import { feedConfig } from "./feed-config.ts";

const root = fileURLToPath(new URL("../", import.meta.url));
const isoDate =
  /^\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\d|3[01])(?:T(?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d(?:\.\d{1,9})?)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d))?$/;

interface Article {
  file: string;
  slug: string;
  title: string;
  description: string;
  date: Date;
  author?: string;
  url: string;
  content: string;
}

function text(value: unknown, field: string): string {
  if (typeof value !== "string" || !value.trim())
    throw new Error(`${field} must be a non-empty string`);
  for (const character of value) {
    const code = character.codePointAt(0)!;
    if (
      (code < 0x20 && ![9, 10, 13].includes(code)) ||
      (code >= 0xd800 && code <= 0xdfff) ||
      code === 0xfffe ||
      code === 0xffff
    ) {
      throw new Error(`${field} contains characters forbidden in XML`);
    }
  }
  return value.trim();
}

function dateValue(value: unknown): Date {
  const valueText = text(value, "date");
  const day = valueText.slice(0, 10);
  const calendar = new Date(`${day}T00:00:00Z`);
  if (
    !isoDate.test(valueText) ||
    !Number.isFinite(calendar.getTime()) ||
    calendar.toISOString().slice(0, 10) !== day
  ) {
    throw new Error(
      "date must be YYYY-MM-DD or ISO 8601 with Z / timezone offset, with a valid calendar date",
    );
  }
  const date = new Date(valueText.length === 10 ? `${valueText}T00:00:00Z` : valueText);
  if (!Number.isFinite(date.getTime())) throw new Error("date could not be parsed");
  return date;
}

// HTML page escaping only. RSS/XML serialization is entirely handled by feed.
function html(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function page(title: string, body: string, url: string, rssUrl: string): string {
  return `<!doctype html>\n<html lang="${html(feedConfig.language)}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${html(title)}</title><link rel="canonical" href="${html(url)}"><link rel="alternate" type="application/rss+xml" title="${html(feedConfig.title)}" href="${html(rssUrl)}"></head><body><main>${body}</main></body></html>\n`;
}

export async function generateFeed({
  sourceDir = join(root, "news"),
  outputDir = join(root, "public"),
  siteUrl = feedConfig.siteUrl,
}: { sourceDir?: string; outputDir?: string; siteUrl?: string } = {}) {
  const base = new URL(siteUrl);
  if (
    !/^https?:$/.test(base.protocol) ||
    base.username ||
    base.password ||
    base.search ||
    base.hash
  ) {
    throw new Error(
      "NEWS_SITE_URL must be an absolute HTTP(S) URL without credentials, query or fragment",
    );
  }
  if (!base.pathname.endsWith("/")) base.pathname += "/";
  const rssUrl = new URL("rss.xml", base).href;
  const indexUrl = new URL("news/", base).href;
  const articles: Article[] = [];
  const slugs = new Map<string, string>();
  const entries = await readdir(sourceDir, { withFileTypes: true });
  for (const entry of entries
    .filter((entry) => entry.name.endsWith(".md"))
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
    const file = join(sourceDir, entry.name);
    try {
      if (!entry.isFile())
        throw new Error("expected a regular Markdown file, not a link or directory");
      const source = await readFile(file, "utf8");
      if (!source.startsWith("---\n") && !source.startsWith("---\r\n"))
        throw new Error("YAML Front Matter is required (--- on its own line)");
      if (!/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/.test(source))
        throw new Error("YAML Front Matter must end with --- on its own line");
      // YAML 1.2 leaves dates as strings, so invalid calendar dates cannot be silently normalized.
      const parsed = matter(source, {
        engines: { yaml: (input: string) => parseYaml(input, { uniqueKeys: true }) },
      });
      const data: Record<string, unknown> = parsed.data;
      const title = text(data.title, "title");
      const description = text(data.description, "description");
      const date = dateValue(data.date);
      const author = data.author === undefined ? undefined : text(data.author, "author");
      const slug = text(data.slug === undefined ? basename(entry.name, ".md") : data.slug, "slug")
        .normalize("NFC")
        .toLowerCase();
      if (
        !/^[\p{L}\p{N}][\p{L}\p{N}_-]*$/u.test(slug) ||
        /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i.test(slug)
      ) {
        throw new Error("slug must be a safe path segment using letters, numbers, - or _");
      }
      if (slugs.has(slug)) throw new Error(`duplicate slug "${slug}" (also in ${slugs.get(slug)})`);
      slugs.set(slug, file);
      const url = new URL(`news/${encodeURIComponent(slug)}/`, base).href;
      const renderer = new Marked({
        async: false,
        renderer: { html: ({ text }) => html(text) },
        walkTokens(token) {
          if (token.type === "link" || token.type === "image") {
            const target = new URL(token.href, url);
            const allowed =
              token.type === "image" ? ["https:", "http:"] : ["https:", "http:", "mailto:"];
            if (!allowed.includes(target.protocol))
              throw new Error(`unsupported Markdown URL protocol: ${target.protocol}`);
            token.href = target.href;
          }
        },
      });
      text(parsed.content, "body");
      const content = renderer.parse(parsed.content, { async: false });
      if (content) text(content, "body");
      articles.push({ file, slug, title, description, date, author, url, content });
    } catch (error) {
      throw new Error(`${file}: ${error instanceof Error ? error.message : String(error)}`, {
        cause: error,
      });
    }
  }
  articles.sort(
    (a, b) =>
      b.date.getTime() - a.date.getTime() || (a.slug < b.slug ? -1 : a.slug > b.slug ? 1 : 0),
  );
  const feed = new Feed({
    ...feedConfig,
    id: rssUrl,
    link: indexUrl,
    updated: articles[0]?.date ?? new Date(0),
    feedLinks: { rss: rssUrl },
  });
  for (const article of articles) {
    feed.addItem({
      ...article,
      id: article.url,
      link: article.url,
      author: article.author ? [{ name: article.author }] : undefined,
    });
  }
  const xml = feed.rss2();
  // Validation completes before replacing generated output. public/news is generator-owned.
  const newsOutput = resolve(outputDir, "news");
  if (newsOutput === resolve(sourceDir))
    throw new Error("sourceDir must not be the generated news directory");
  await mkdir(outputDir, { recursive: true });
  await rm(newsOutput, { recursive: true, force: true });
  await mkdir(newsOutput, { recursive: true });
  for (const article of articles) {
    const articleDir = join(newsOutput, article.slug);
    await mkdir(articleDir);
    await writeFile(
      join(articleDir, "index.html"),
      page(
        article.title,
        `<nav><a href="${html(indexUrl)}">News</a> · <a href="${html(rssUrl)}">RSS</a></nav><article><h1>${html(article.title)}</h1><time datetime="${article.date.toISOString()}">${article.date.toISOString().slice(0, 10)}</time>${article.content}</article>`,
        article.url,
        rssUrl,
      ),
    );
  }
  await writeFile(
    join(newsOutput, "index.html"),
    page(
      feedConfig.title,
      `<h1>${html(feedConfig.title)}</h1><p><a href="${html(rssUrl)}">RSS</a></p><ul>${articles.map((article) => `<li><a href="${html(article.url)}">${html(article.title)}</a> <time datetime="${article.date.toISOString()}">${article.date.toISOString().slice(0, 10)}</time></li>`).join("")}</ul>`,
      indexUrl,
      rssUrl,
    ),
  );
  await writeFile(join(outputDir, "rss.xml"), xml);
  return { xml, articles };
}

if (import.meta.main) {
  try {
    const result = await generateFeed();
    console.log(`Generated public/rss.xml and public/news/ (${result.articles.length} articles)`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
