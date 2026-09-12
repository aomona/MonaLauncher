import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { test, expect } from "@playwright/test";

test("generated feed is valid RSS 2.0 XML with readable Japanese and HTML content", async ({
  page,
}) => {
  execFileSync(process.execPath, ["scripts/generate-feed.ts"]);
  const xml = await readFile("public/rss.xml", "utf8");
  const result = await page.evaluate((source) => {
    const doc = new DOMParser().parseFromString(source, "application/xml");
    const channel = doc.querySelector("channel");
    const value = (element: Element | null, tag: string) =>
      element?.getElementsByTagName(tag)[0]?.textContent ?? "";
    return {
      error: doc.querySelector("parsererror")?.textContent,
      version: doc.documentElement.getAttribute("version"),
      title: value(channel, "title"),
      language: value(channel, "language"),
      copyright: value(channel, "copyright"),
      updated: value(channel, "lastBuildDate"),
      self: doc
        .getElementsByTagNameNS("http://www.w3.org/2005/Atom", "link")[0]
        ?.getAttribute("href"),
      items: Array.from(doc.querySelectorAll("item")).map((item) => ({
        title: value(item, "title"),
        link: value(item, "link"),
        guid: value(item, "guid"),
        date: value(item, "pubDate"),
        description: value(item, "description"),
        content:
          item.getElementsByTagNameNS("http://purl.org/rss/1.0/modules/content/", "encoded")[0]
            ?.textContent ?? "",
      })),
    };
  }, xml);
  expect(result.error).toBeUndefined();
  expect(result.version).toBe("2.0");
  expect(result.title).toBe("MonaLauncher News");
  expect(result.language).toBe("ja");
  expect(result.copyright).toBeTruthy();
  expect(result.self).toMatch(/^https?:\/\/.+\/rss.xml$/);
  expect(Number.isFinite(Date.parse(result.updated))).toBe(true);
  const dates = result.items.map((item) => Date.parse(item.date));
  expect(dates).toEqual([...dates].sort((a, b) => b - a));
  for (const item of result.items) {
    expect(item.title).toBeTruthy();
    expect(item.link).toMatch(/^https?:\/\/.+\/news\/.+\/$/);
    expect(item.guid).toBe(item.link);
    expect(Number.isFinite(Date.parse(item.date))).toBe(true);
    expect(item.description).toBeTruthy();
    expect(item.content).toBeTruthy();
  }
  const example = result.items.find((item) => item.link.endsWith("/news/feed-example/"));
  if (example) {
    expect(example.title).toBe("MonaLauncherニュース配信のサンプル");
    expect(example.content).toContain("<strong>RSS生成の確認用サンプル</strong>");
    expect(example.content).toContain("日本語の本文");
  }
});
