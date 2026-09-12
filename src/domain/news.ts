export interface NewsEntry {
  id: string;
  kind: "news" | "javaPatchNotes" | "monaLauncher";
  title: string;
  summary: string;
  category: string;
  date: string;
  articleUrl: string;
  imageUrl: string | null;
  contentHtml?: string | null;
  author?: string | null;
}

export interface NewsFeed {
  entries: NewsEntry[];
  fetchedAt: number;
  cached: boolean;
  warning: string | null;
}
