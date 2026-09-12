import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useRef, useState } from "react";
import { hasTauriRuntime } from "../../lib/tauri";

import type { NewsFeed } from "../../domain/news";

export function useNews() {
  const [feed, setFeed] = useState<NewsFeed | null>(null);
  const [loading, setLoading] = useState(hasTauriRuntime);
  const [cached, setCached] = useState(false);
  const [error, setError] = useState("");
  const request = useRef({ id: 0 });
  const refresh = useCallback(async (initial = false) => {
    if (!hasTauriRuntime()) return;
    const id = ++request.current.id;
    setLoading(true);
    setError("");
    if (initial) {
      const saved = await invoke<NewsFeed | null>("cached_minecraft_news").catch(() => null);
      if (request.current.id !== id) return;
      if (saved) {
        setFeed(saved);
        setCached(true);
      }
    }
    try {
      const result = await invoke<NewsFeed>("fetch_minecraft_news");
      if (request.current.id === id) {
        setFeed(result);
        setCached(result.cached);
      }
    } catch (cause) {
      if (request.current.id === id) {
        setError(String(cause));
        setCached(true);
      }
    } finally {
      if (request.current.id === id) setLoading(false);
    }
  }, []);
  useEffect(() => {
    const activeRequest = request.current;
    void refresh(true);
    return () => {
      activeRequest.id++;
    };
  }, [refresh]);
  return { feed, loading, cached, error, refresh, available: hasTauriRuntime() };
}

export type NewsController = ReturnType<typeof useNews>;

export function useNewsArticle(url: string) {
  const [error, setError] = useState("");
  const [opening, setOpening] = useState(false);
  async function open() {
    setError("");
    setOpening(true);
    try {
      await openUrl(url);
    } catch {
      setError("既定ブラウザで記事を開けませんでした。もう一度お試しください。");
    } finally {
      setOpening(false);
    }
  }
  return { open, error, opening };
}
