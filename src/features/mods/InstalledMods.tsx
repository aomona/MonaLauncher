import type { Launcher } from "../../app/useLauncher";
import { Button } from "../../components/Button";
import { Empty } from "../../components/Empty";
import { ErrorMessage } from "../../components/ErrorMessage";

type InstalledModsModel = Pick<
  Launcher,
  | "busy"
  | "installedMods"
  | "installedModsLoading"
  | "isRunning"
  | "modError"
  | "modOperationActive"
  | "modSuccess"
  | "setModRemovalTarget"
>;
export function InstalledMods({ launcher: l }: { launcher: InstalledModsModel }) {
  return (
    <>
      <ErrorMessage>{l.modError}</ErrorMessage>
      {l.modSuccess && <output className="my-4">{l.modSuccess}</output>}
      {l.installedModsLoading ? (
        <output>Modを読み込んでいます…</output>
      ) : !l.installedMods.length ? (
        <Empty title={l.modError ? "Mod一覧を取得できませんでした" : "管理対象のModはありません"}>
          <p>Modrinthで導入・管理しているModがここに表示されます。</p>
        </Empty>
      ) : (
        <div>
          {l.installedMods.map((mod) => (
            <div key={mod.projectId} className="mod-row">
              <div className="min-w-0 flex-1">
                <p className="wrap-anywhere text-navigation">{mod.title}</p>
                <p className="mt-1 wrap-anywhere font-mono text-small text-text-secondary">
                  {mod.versionNumber} · {mod.fileName}
                </p>
                <p className="mt-1 text-caption text-text-secondary">
                  {mod.direct ? "直接追加" : "必須依存"}
                </p>
              </div>
              {mod.direct && (
                <Button
                  disabled={l.isRunning || l.modOperationActive || Boolean(l.busy)}
                  onClick={() => l.setModRemovalTarget(mod)}
                >
                  削除…
                </Button>
              )}
            </div>
          ))}
        </div>
      )}
    </>
  );
}
