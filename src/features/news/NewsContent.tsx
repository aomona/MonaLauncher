import { useState } from "react";
import type { NewsEntry } from "../../domain/news";
import { ExternalLink } from "lucide-react";
import { Button } from "../../components/Button";
import { ErrorMessage } from "../../components/ErrorMessage";
import { useNewsArticle, type NewsController } from "./useNews";

function NewsImage({ url }: { url: string }) {
  const [failed, setFailed] = useState(false);
  return failed ? null : (
    <img
      src={url}
      alt=""
      loading="lazy"
      referrerPolicy="no-referrer"
      className="news-thumbnail"
      onError={() => setFailed(true)}
    />
  );
}

function NewsRow({ entry, heading: Heading }: { entry: NewsEntry; heading: "h2" | "h3" }) {
  const link = useNewsArticle(entry.articleUrl);
  return (
    <li className="news-row">
      {entry.imageUrl && <NewsImage key={entry.imageUrl} url={entry.imageUrl} />}
      <div className="min-w-0 flex-1">
        <Heading className="mb-2 text-navigation">
          <Button
            tone="ghost"
            className="news-title"
            disabled={link.opening}
            title="既定ブラウザで原文を開く"
            onClick={() => void link.open()}
          >
            {entry.title}{" "}
            <ExternalLink size={14} aria-hidden="true" className="inline-block align-middle" />
          </Button>
        </Heading>
        <p className="mb-2 line-clamp-2 wrap-anywhere text-text-secondary">{entry.summary}</p>
        <p className="text-small text-text-secondary">
          Minecraft · {entry.category} ·{" "}
          <time dateTime={entry.date}>{entry.date.slice(0, 10)}</time>
        </p>
        <ErrorMessage>{link.error}</ErrorMessage>
      </div>
    </li>
  );
}

export function NewsContent({
  news,
  limit,
  kind,
}: {
  news: NewsController;
  limit?: number;
  kind?: NewsEntry["kind"];
}) {
  const [visibleCount, setVisibleCount] = useState(30);
  const filtered = news.feed?.entries.filter((entry) => !kind || entry.kind === kind) ?? [];
  const entries = filtered.slice(0, limit ?? visibleCount);
  return (
    <>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <output className="text-small text-text-secondary">
          {!news.available
            ? "ニュースの取得はデスクトップアプリで利用できます。"
            : news.loading
              ? news.feed
                ? "保存済みの記事を表示し、更新しています…"
                : "ニュースを取得しています…"
              : news.feed
                ? `${news.cached ? "キャッシュ · " : ""}取得日時: ${new Date(news.feed.fetchedAt).toLocaleString("ja-JP")}`
                : "ニュースは未取得です。"}
          {news.loading &&
            news.feed &&
            ` · 取得日時: ${new Date(news.feed.fetchedAt).toLocaleString("ja-JP")}`}
        </output>
        {news.available && (
          <Button tone="ghost" disabled={news.loading} onClick={() => void news.refresh()}>
            {news.error ? "再試行" : "更新"}
          </Button>
        )}
      </div>
      <ErrorMessage>{news.error}</ErrorMessage>
      {news.feed?.warning && (
        <p className="mb-4 whitespace-pre-line text-small text-text-secondary">
          {news.feed.warning}
        </p>
      )}
      {news.feed && entries.length === 0 && (
        <p className="text-text-secondary">配信されているニュースはありません。</p>
      )}
      <ul className="news-list" aria-label="Minecraftのニュース">
        {entries.map((entry) => (
          <NewsRow key={entry.id} entry={entry} heading={limit ? "h3" : "h2"} />
        ))}
      </ul>
      {!limit && entries.length < filtered.length && (
        <Button className="mt-4" onClick={() => setVisibleCount((count) => count + 30)}>
          もっと表示（{entries.length} / {filtered.length}件）
        </Button>
      )}
    </>
  );
}
