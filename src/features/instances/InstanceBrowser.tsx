import { Search, SlidersHorizontal } from "lucide-react";
import type { ReactNode } from "react";
import type { Launcher } from "../../app/useLauncher";
import { Button } from "../../components/Button";
import { Empty } from "../../components/Empty";

import { InstanceList, type OpenInstance } from "./InstanceList";
import { filterInstances, type InstanceFilters } from "./useInstanceFilters";
export function InstanceBrowser({
  launcher,
  history,
  filters,
  openInstance,
  createButton,
}: {
  launcher: Launcher;
  history: Record<string, number>;
  filters: InstanceFilters;
  openInstance: OpenInstance;
  createButton: ReactNode;
}) {
  const { instances } = launcher;
  const {
    query,
    setQuery,
    filtersOpen,
    setFiltersOpen,
    version,
    setVersion,
    loader,
    setLoader,
    sort,
    setSort,
  } = filters;
  const filtered = filterInstances(instances, history, filters);
  return (
    <>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <label className="relative min-w-0 flex-1">
          <Search
            size={16}
            className="pointer-events-none absolute top-3 left-3 text-text-secondary"
          />
          <span className="sr-only">インスタンスを検索</span>
          <input
            className="pl-10"
            placeholder="Search instances…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <Button aria-expanded={filtersOpen} onClick={() => setFiltersOpen(!filtersOpen)}>
          <SlidersHorizontal size={16} />
          Filter
          {(version || loader) && ` (${Number(Boolean(version)) + Number(Boolean(loader))})`}
        </Button>
        <label>
          <span className="sr-only">並び順</span>
          <select value={sort} onChange={(event) => setSort(event.target.value)}>
            <option value="recent">Last Played</option>
            <option value="name">Name</option>
          </select>
        </label>
      </div>
      {filtersOpen && (
        <div className="mb-4 flex flex-wrap items-end gap-4">
          <label className="field">
            Minecraft version
            <select value={version} onChange={(event) => setVersion(event.target.value)}>
              <option value="">All versions</option>
              {[...new Set(instances.map((item) => item.versionId))].map((id) => (
                <option key={id}>{id}</option>
              ))}
            </select>
          </label>
          <label className="field">
            Mod Loader
            <select value={loader} onChange={(event) => setLoader(event.target.value)}>
              <option value="">All loaders</option>
              {[...new Set(instances.map((item) => item.modLoader.type))].map((id) => (
                <option key={id} value={id}>
                  {id === "fabric" ? "Fabric" : "Vanilla"}
                </option>
              ))}
            </select>
          </label>
          <Button
            tone="ghost"
            onClick={() => {
              setVersion("");
              setLoader("");
            }}
          >
            Clear filters
          </Button>
        </div>
      )}
      {launcher.instancesLoading ? (
        <output className="py-16">インスタンスを読み込んでいます…</output>
      ) : !instances.length ? (
        <Empty
          title={
            launcher.error ? "インスタンスを取得できませんでした" : "インスタンスはまだありません"
          }
          action={
            launcher.error ? (
              <Button
                onClick={() =>
                  void launcher
                    .refreshInstances()
                    .catch((cause) => launcher.setError(String(cause)))
                }
              >
                再試行
              </Button>
            ) : (
              createButton
            )
          }
        >
          独立したMinecraft環境を追加してください。
        </Empty>
      ) : !filtered.length ? (
        <Empty
          title="一致するインスタンスがありません"
          action={
            <Button
              onClick={() => {
                setQuery("");
                setVersion("");
                setLoader("");
              }}
            >
              検索条件をクリア
            </Button>
          }
        >
          名前やフィルターを変更してください。
        </Empty>
      ) : (
        <>
          <p className="mb-2 text-small text-text-secondary">{filtered.length} instances</p>
          <InstanceList items={filtered} launcher={launcher} openInstance={openInstance} />
        </>
      )}
    </>
  );
}
