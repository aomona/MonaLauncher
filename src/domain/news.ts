export interface NewsEntry {
  id: string;
  kind: "news" | "javaPatchNotes";
  title: string;
  summary: string;
  category: string;
  date: string;
  articleUrl: string;
  imageUrl: string | null;
}

export interface NewsFeed {
  entries: NewsEntry[];
  fetchedAt: number;
  cached: boolean;
  warning: string | null;
}
