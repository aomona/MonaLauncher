import { Search } from "lucide-react";
import type { Launcher } from "../../app/useLauncher";
import { Button, Dialog, Empty, ErrorMessage, Progress } from "../../components/ui";

type ModCatalogModel = Pick<
  Launcher,
  | "currentModInstallProgress"
  | "installModrinthMod"
  | "installedProjectIds"
  | "isRunning"
  | "modError"
  | "modInstallPercent"
  | "modInstallingProjectId"
  | "modOperationActive"
  | "modQuery"
  | "modSearch"
  | "modSearchLoading"
  | "modSuccess"
  | "runModSearch"
  | "selected"
  | "setModQuery"
  | "setShowMods"
>;
export function ModCatalog({ launcher: l }: { launcher: ModCatalogModel }) {
  return (
    <Dialog
      title="Add mods"
      large
      onClose={() => l.setShowMods(false)}
      footer={
        l.currentModInstallProgress ? (
          <Progress
            label={l.currentModInstallProgress.message}
            value={l.currentModInstallProgress.total > 0 ? l.modInstallPercent : undefined}
          />
        ) : undefined
      }
    >
      <div className="dialog-body flex-1">
        <form
          className="mb-6 flex flex-wrap gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void l.runModSearch(l.selected!.id, l.modQuery, 0);
          }}
        >
          <label className="min-w-0 flex-1">
            <span className="sr-only">Modrinthを検索</span>
            <input
              value={l.modQuery}
              onChange={(event) => l.setModQuery(event.target.value)}
              placeholder="Search Modrinth…"
            />
          </label>
          <Button type="submit" disabled={l.modSearchLoading}>
            <Search size={16} />
            検索
          </Button>
        </form>
        <p className="mb-4 text-small text-text-secondary">
          Minecraft {l.selected!.versionId} / Fabricに対応するModを検索します。
        </p>
        <ErrorMessage>{l.modError}</ErrorMessage>
        {l.modSuccess && <output className="my-4">{l.modSuccess}</output>}
        {l.modSearchLoading && <output>検索しています…</output>}
        {l.modSearch?.hits.map((mod) => (
          <div className="mod-row" key={mod.projectId}>
            <div className="min-w-0 flex-1">
              <h3 className="text-navigation">{mod.title}</h3>
              <p className="mt-1 wrap-anywhere text-text-secondary">{mod.description}</p>
              <p className="mt-2 text-caption text-text-secondary">
                {mod.author} · {mod.downloads.toLocaleString("ja-JP")} downloads
              </p>
            </div>
            <Button
              disabled={
                l.modOperationActive || l.isRunning || l.installedProjectIds.has(mod.projectId)
              }
              onClick={() => void l.installModrinthMod(mod.projectId)}
            >
              {l.modInstallingProjectId === mod.projectId
                ? "導入中…"
                : l.installedProjectIds.has(mod.projectId)
                  ? "導入済み"
                  : "導入"}
            </Button>
          </div>
        ))}
        {!l.modSearchLoading && l.modSearch?.hits.length === 0 && (
          <Empty title="一致するModがありません">検索語を変更してください。</Empty>
        )}
        {l.modSearch && (
          <div className="mt-6 flex flex-wrap items-center gap-4">
            <Button
              disabled={l.modSearchLoading || l.modSearch.offset === 0}
              onClick={() =>
                void l.runModSearch(
                  l.selected!.id,
                  l.modQuery,
                  Math.max(0, l.modSearch!.offset - l.modSearch!.limit),
                )
              }
            >
              前へ
            </Button>
            <span className="text-small">{l.modSearch.totalHits}件</span>
            <Button
              disabled={
                l.modSearchLoading ||
                l.modSearch.offset + l.modSearch.limit >= l.modSearch.totalHits
              }
              onClick={() =>
                void l.runModSearch(
                  l.selected!.id,
                  l.modQuery,
                  l.modSearch!.offset + l.modSearch!.limit,
                )
              }
            >
              次へ
            </Button>
          </div>
        )}
      </div>
    </Dialog>
  );
}
