import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

if (!["darwin", "linux"].includes(process.platform) || !["arm64", "x64"].includes(process.arch)) {
  throw new Error("The LWJGL probe supports macOS/Linux arm64/x64 only.");
}
const manifest = JSON.parse(
  await readFile(new URL("./lwjgl-artifacts.json", import.meta.url), "utf8"),
);
const os = process.platform === "darwin" ? "macos" : "linux";
const classifier = `natives-${os}${process.arch === "arm64" ? "-arm64" : ""}`;
const directory = new URL(`./.cache/lwjgl-${os}-${process.arch}/`, import.meta.url);
await mkdir(directory, { recursive: true });
for (const artifact of manifest.artifacts) {
  if (artifact.file.includes("-natives-") && !artifact.file.endsWith(`-${classifier}.jar`))
    continue;
  const target = new URL(artifact.file, directory);
  const digest = (data) => createHash("sha256").update(data).digest("hex");
  const cached = await readFile(target).catch((error) => {
    if (error.code === "ENOENT") return undefined;
    throw error;
  });
  if (cached && digest(cached) === artifact.sha256) continue;
  const response = await fetch(artifact.url, { signal: AbortSignal.timeout(30_000) });
  if (!response.ok) throw new Error(`${response.status}: ${artifact.url}`);
  const data = Buffer.from(await response.arrayBuffer());
  if (digest(data) !== artifact.sha256) throw new Error(`SHA-256 mismatch: ${artifact.file}`);
  await writeFile(target, data);
}
console.log(fileURLToPath(directory));
