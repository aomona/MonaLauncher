import { useMemo, type MouseEvent, type RefObject } from "react";
import { Dialog } from "../../components/Dialog";
import { ErrorMessage } from "../../components/ErrorMessage";
import type { NewsEntry } from "../../domain/news";
import { sanitizeArticle } from "./sanitizeArticle";
import { useNewsArticle } from "./useNews";

export function NewsArticleDialog({
  entry,
  onClose,
  trigger,
}: {
  entry: NewsEntry;
  onClose: () => void;
  trigger: RefObject<HTMLButtonElement | null>;
}) {
  // Keep the markup prop stable during link status updates so React preserves the focused anchor.
  const markup = useMemo(
    () => ({ __html: sanitizeArticle(entry.contentHtml ?? "") }),
    [entry.contentHtml],
  );
  const link = useNewsArticle(entry.articleUrl);
  function openLink(event: MouseEvent<HTMLElement>) {
    const anchor = event.target instanceof Element ? event.target.closest("a[href]") : null;
    if (!anchor || !event.currentTarget.contains(anchor)) return;
    event.preventDefault();
    if (event.type === "auxclick" && event.button !== 1) return;
    const url = new URL(anchor.getAttribute("href")!);
    if (url.protocol === "https:" && !url.username && !url.password) void link.open(url.href);
  }
  return (
    <Dialog
      title={entry.title}
      onClose={onClose}
      finalFocus={trigger}
      className="news-article-dialog"
    >
      <div className="dialog-body">
        <p className="mb-6 text-small text-text-secondary">
          MonaLauncher · <time dateTime={entry.date}>{entry.date.slice(0, 10)}</time>
          {entry.author && ` · ${entry.author}`}
        </p>
        <ErrorMessage>{link.error}</ErrorMessage>
        {markup.__html ? (
          <article
            className="news-article-body"
            aria-label="記事本文"
            onClickCapture={openLink}
            onAuxClick={openLink}
            dangerouslySetInnerHTML={markup}
          />
        ) : (
          <p className="text-text-secondary">
            記事の本文を表示できません。ニュースを更新して再度お試しください。
          </p>
        )}
      </div>
    </Dialog>
  );
}
