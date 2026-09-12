import { useState } from "react";
import { Tabs } from "../../components/Tabs";
import { Empty } from "../../components/Empty";
import { NewsContent } from "./NewsContent";
import type { NewsController } from "./useNews";

export function NewsPage({ news }: { news: NewsController }) {
  const [tab, setTab] = useState("All");
  return (
    <Tabs
      id="news"
      label="ニュースの配信元"
      tabs={["All", "Minecraft", "Java Patch Notes", "MonaLauncher"]}
      active={tab}
      onChange={setTab}
      panelProps={{ className: "pt-6" }}
    >
      {tab === "MonaLauncher" ? (
        <Empty title="MonaLauncherのニュースは未配信です">
          <p>MonaLauncher独自のお知らせの配信元は、まだ設定されていません。</p>
        </Empty>
      ) : (
        <NewsContent
          key={tab}
          news={news}
          kind={
            tab === "Minecraft" ? "news" : tab === "Java Patch Notes" ? "javaPatchNotes" : undefined
          }
        />
      )}
    </Tabs>
  );
}
