import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { generateFeed } from "./generate-feed.ts";

const article = (
  fields = "title: Test\ndate: 2026-09-12\ndescription: Summary",
  body = "日本語の**本文**。",
) => `---\n${fields}\n---\n\n${body}\n`;

async function fixture() {
  const dir = await mkdtemp(join(tmpdir(), "mona-feed-"));
  const sourceDir = join(dir, "source");
  const outputDir = join(dir, "public");
  await mkdir(sourceDir);
  return { dir, sourceDir, outputDir, siteUrl: "https://example.test/project/" };
}

test("RSS contains HTML, absolute URLs, UTC dates, stable ordering and stable metadata", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.dir, { recursive: true, force: true }));
  await writeFile(
    join(f.sourceDir, "b.md"),
    article(
      'title: "日本語 & <更新>"\ndate: 2026-09-12T12:00:00+09:00\ndescription: "説明 & 本文"\nauthor: Author',
      "**太字**と[リンク](../a/)\n\n![画像](./image.png)\n\n<script>alert(1)</script>",
    ),
  );
  await writeFile(
    join(f.sourceDir, "a.md"),
    article("title: Equal time\ndate: 2026-09-12T03:00:00Z\ndescription: Same instant"),
  );
  await writeFile(
    join(f.sourceDir, "old.md"),
    article("title: Old\ndate: 2026-09-11\ndescription: Older\nslug: older"),
  );
  const result = await generateFeed(f);
  assert.deepEqual(
    result.articles.map((item) => item.slug),
    ["a", "b", "older"],
  );
  assert.equal(result.articles[0].date.toISOString(), "2026-09-12T03:00:00.000Z");
  assert.match(result.xml, /<rss[^>]*version="2.0"/);
  assert.match(result.xml, /<lastBuildDate>Sat, 12 Sep 2026 03:00:00 GMT<\/lastBuildDate>/);
  assert.match(result.xml, /<content:encoded><!\[CDATA\[/);
  assert.match(result.xml, /<strong>太字<\/strong>/);
  assert.match(result.xml, /https:\/\/example.test\/project\/news\/a\//);
  assert.match(result.xml, /https:\/\/example.test\/project\/news\/b\/image.png/);
  assert.doesNotMatch(result.xml, /<script>/);
  assert.equal(result.xml, (await generateFeed(f)).xml);
  const html = await readFile(join(f.outputDir, "news", "b", "index.html"), "utf8");
  assert.match(html, /日本語 &amp; &lt;更新&gt;/);
  assert.match(html, /<strong>太字<\/strong>/);

  const moduleUrl = new URL("./generate-feed.ts", import.meta.url).href;
  const script = `import {generateFeed} from ${JSON.stringify(moduleUrl)}; process.stdout.write((await generateFeed(${JSON.stringify(f)})).xml);`;
  const utc = execFileSync(process.execPath, ["--input-type=module", "-e", script], {
    env: { ...process.env, TZ: "UTC" },
  });
  const tokyo = execFileSync(process.execPath, ["--input-type=module", "-e", script], {
    env: { ...process.env, TZ: "Asia/Tokyo" },
  });
  assert.deepEqual(utc, tokyo);

  await rm(join(f.sourceDir, "old.md"));
  await generateFeed(f);
  await assert.rejects(readFile(join(f.outputDir, "news", "older", "index.html")), {
    code: "ENOENT",
  });
});

for (const [label, fields] of [
  ["missing title", "date: 2026-09-12\ndescription: Test"],
  ["missing date", "title: Test\ndescription: Test"],
  ["missing description", "title: Test\ndate: 2026-09-12"],
  ["unparseable date", "title: Test\ndate: yesterday\ndescription: Test"],
  ["invalid unquoted calendar date", "title: Test\ndate: 2026-02-30\ndescription: Test"],
  ["invalid quoted calendar date", 'title: Test\ndate: "2026-02-30T00:00:00Z"\ndescription: Test'],
  ["ambiguous timezone", "title: Test\ndate: 2026-09-12T12:00:00\ndescription: Test"],
  ["non-string title", "title: 42\ndate: 2026-09-12\ndescription: Test"],
  ["unsafe slug", "title: Test\ndate: 2026-09-12\ndescription: Test\nslug: ../outside"],
  ["YAML duplicate key", "title: One\ntitle: Two\ndate: 2026-09-12\ndescription: Test"],
]) {
  test(`rejects ${label} with the filename and preserves previous output`, async (t) => {
    const f = await fixture();
    t.after(() => rm(f.dir, { recursive: true, force: true }));
    await writeFile(join(f.sourceDir, "good.md"), article());
    const previous = (await generateFeed(f)).xml;
    await writeFile(join(f.sourceDir, "invalid.md"), article(fields));
    await assert.rejects(generateFeed(f), /invalid\.md:/);
    assert.equal(await readFile(join(f.outputDir, "rss.xml"), "utf8"), previous);
  });
}

test("duplicate slugs, malformed front matter and unsafe Markdown fail explicitly", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.dir, { recursive: true, force: true }));
  await writeFile(join(f.sourceDir, "one.md"), article());
  await writeFile(
    join(f.sourceDir, "two.md"),
    article("title: Two\ndate: 2026-09-12\ndescription: Other\nslug: ONE"),
  );
  await assert.rejects(generateFeed(f), /two\.md: duplicate slug "one".*one\.md/);
  for (const source of [
    "No front matter",
    "---\ntitle: Test\ndate: 2026-09-12\ndescription: No closing delimiter",
    article(undefined, ""),
    "---javascript\nthrow new Error('must not execute')\n---",
    article(undefined, "[bad](javascript:alert%281%29)"),
  ]) {
    await writeFile(join(f.sourceDir, "two.md"), source);
    await assert.rejects(generateFeed(f), /two\.md:/);
  }
});

test("empty feeds are deterministic and preserve project paths", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.dir, { recursive: true, force: true }));
  const output = await generateFeed({ ...f, siteUrl: "https://example.test/project" });
  assert.match(output.xml, /Thu, 01 Jan 1970 00:00:00 GMT/);
  assert.match(output.xml, /https:\/\/example.test\/project\/rss.xml/);
  assert.equal(output.xml, (await generateFeed(f)).xml);
  await assert.rejects(generateFeed({ ...f, siteUrl: "file:///tmp/feed" }), /NEWS_SITE_URL/);
});
