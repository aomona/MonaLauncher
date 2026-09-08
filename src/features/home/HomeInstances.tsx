import type { ReactNode } from "react";
import type { Page } from "../../app/navigation";
import type { Launcher } from "../../app/useLauncher";
import { Button, Empty } from "../../components/ui";
import type { MinecraftInstance } from "../../domain/launcher";

import { InstanceList, type OpenInstance } from "../instances/InstanceList";
export type HomeInstancesProps = {
  launcher: Launcher;
  playing: MinecraftInstance[];
  recent: MinecraftInstance[];
  openInstance: OpenInstance;
  createButton: ReactNode;
  navigate: (page: Page) => void;
};
export function HomeInstances({
  launcher,
  playing,
  recent,
  openInstance,
  createButton,
  navigate,
}: HomeInstancesProps) {
  const { instances } = launcher;
  return (
    <>
      {playing.length > 0 && (
        <section className="mb-8">
          <h2 className="mb-4 text-section-title">Now Playing</h2>
          <InstanceList items={playing} launcher={launcher} openInstance={openInstance} />
        </section>
      )}
      {launcher.instancesLoading ? (
        <output>インスタンスを読み込んでいます…</output>
      ) : instances.length === 0 ? (
        <Empty title="最初のインスタンスを作成" action={createButton}>
          <p>MinecraftのバージョンとMod Loaderを選んで、独立した環境を作成できます。</p>
        </Empty>
      ) : (
        <section className="mb-8">
          <div className="mb-4 flex items-center justify-between gap-4">
            <h2 className="text-section-title">Recent Instances</h2>
            <Button tone="ghost" onClick={() => navigate("Instances")}>
              View all
            </Button>
          </div>
          {recent.length ? (
            <InstanceList
              items={recent.slice(0, 3)}
              launcher={launcher}
              openInstance={openInstance}
            />
          ) : (
            <p className="text-text-secondary">
              最近使用した履歴はありません。Instancesから起動できます。
            </p>
          )}
        </section>
      )}
    </>
  );
}
