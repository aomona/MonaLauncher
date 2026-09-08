import { useState } from "react";
import type { MinecraftInstance } from "../../domain/launcher";

export function useInstanceFilters() {
  const [query, setQuery] = useState("");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [version, setVersion] = useState("");
  const [loader, setLoader] = useState("");
  const [sort, setSort] = useState("recent");
  return {
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
  };
}
export type InstanceFilters = ReturnType<typeof useInstanceFilters>;
export function filterInstances(
  instances: MinecraftInstance[],
  history: Record<string, number>,
  filters: InstanceFilters,
) {
  const { query, version, loader, sort } = filters;
  const filtered = instances
    .filter(
      (item) =>
        `${item.name} ${item.versionId}`.toLowerCase().includes(query.toLowerCase()) &&
        (!version || item.versionId === version) &&
        (!loader || item.modLoader.type === loader),
    )
    .sort(
      (a, b) =>
        (sort === "recent" ? (history[b.id] ?? 0) - (history[a.id] ?? 0) : 0) ||
        a.name.localeCompare(b.name, "ja") ||
        a.id.localeCompare(b.id),
    );
  return filtered;
}
