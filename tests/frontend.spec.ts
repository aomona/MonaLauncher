import { test, expect, type Page } from "@playwright/test";
import type { InstancePermissions } from "../src/domain/launcher";

async function mockDesktop(
  page: Page,
  count = 2,
  platform: "windows" | "macos" | "linux" | "unsupported" = "windows",
  initialPermissions: Partial<InstancePermissions> = {},
) {
  await page.addInitScript(
    ({ count, platform, initialPermissions }) => {
      const callbacks: Record<number, (data: unknown) => void> = {};
      const handlers: Record<string, number[]> = {};
      let counter = 0;
      const instances = [
        {
          id: "survival",
          name: "Survival",
          versionId: "1.21.1",
          javaPath: "C:\\Java\\bin\\java.exe",
          gameDirectory: "C:\\Minecraft\\Survival",
          sandboxed: true,
          permissions: {
            gameWrite: true,
            narrator: true,
            network: false,
            audioOutput: true,
            microphone: false,
            clipboard: false,
            worldsWrite: true,
            screenshotsWrite: true,
            resourcePacksWrite: true,
            shaderPacksWrite: true,
            modsWrite: true,
            configWrite: true,
            logsWrite: true,
            skinCache: true,
            desktopIntegration: true,
            graphicsCache: true,
          },
          demo: false,
          modLoader: { type: "fabric", version: "0.16.0" },
        },
        {
          id: "creative",
          name: "とても長い日本語のインスタンス名で折り返しと操作ボタンへの到達性を確認する環境",
          versionId: "1.20.4",
          javaPath: "C:\\Java\\bin\\java.exe",
          gameDirectory: "C:\\Minecraft\\Creative",
          sandboxed: true,
          permissions: {
            gameWrite: true,
            narrator: true,
            network: false,
            audioOutput: true,
            microphone: false,
            clipboard: false,
            worldsWrite: true,
            screenshotsWrite: true,
            resourcePacksWrite: true,
            shaderPacksWrite: true,
            modsWrite: true,
            configWrite: true,
            logsWrite: true,
            skinCache: true,
            desktopIntegration: true,
            graphicsCache: true,
          },
          demo: false,
          modLoader: { type: "vanilla" },
        },
      ];
      Object.assign(instances[0].permissions, initialPermissions);
      for (let i = 2; i < count; i++)
        instances.push({ ...instances[0], id: `fixture-${i}`, name: `Instance ${i}` });
      const state = {
        calls: [] as string[],
        newsFeed: {
          entries: [] as {
            id: string;
            title: string;
            summary: string;
            category: string;
            date: string;
            articleUrl: string;
            imageUrl: string | null;
          }[],
          fetchedAt: 1789185600000,
          cached: false,
          warning: null,
        },
        failNews: false,
        delayNews: false,
        finishNews: null as (() => void) | null,
        articleUrl: "",
        failOpenArticle: false,
        failRename: false,
        delayRename: false,
        finishRename: null as (() => void) | null,
        failPermissions: false,
        delayPermissions: false,
        finishPermissions: null as (() => void) | null,
        failStop: false,
        delayModSearch: false,
        rejectSearch: null as (() => void) | null,
        emit: (event: string, payload: unknown) => {
          for (const id of handlers[event] ?? []) callbacks[id]?.({ event, payload, id });
        },
      };
      Object.assign(window, {
        isTauri: true,
        __test: state,
        __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
        __TAURI_INTERNALS__: {
          transformCallback: (fn: (data: unknown) => void) => {
            callbacks[++counter] = fn;
            return counter;
          },
          unregisterCallback: (id: number) => {
            delete callbacks[id];
          },
          invoke: async (command: string, args: Record<string, string | number>) => {
            state.calls.push(command);
            switch (command) {
              case "plugin:event|listen":
                (handlers[args.event] ??= []).push(Number(args.handler));
                return args.handler;
              case "plugin:event|unlisten":
                for (const key in handlers)
                  handlers[key] = handlers[key].filter((id) => id !== args.eventId);
                return;
              case "cached_minecraft_news":
                return null;
              case "fetch_minecraft_news":
                if (state.delayNews)
                  await new Promise<void>((resolve) => {
                    state.finishNews = resolve;
                  });
                if (state.failNews) throw new Error("ニュースの取得に失敗しました");
                return state.newsFeed;
              case "list_minecraft_instances":
                return [...instances];
              case "microsoft_auth_status":
                return { configured: true, authorized: false };
              case "begin_microsoft_sign_in":
                return {
                  sessionId: "test-session",
                  userCode: "TEST-CODE",
                  verificationUri: "https://www.microsoft.com/link",
                  expiresIn: 900,
                  interval: 0.01,
                };
              case "plugin:opener|open_url":
                if (state.failOpenArticle) throw new Error("open failed");
                state.articleUrl = String(args.url);
                return;
              case "sign_out_microsoft":
                return;
              case "poll_microsoft_sign_in":
                return { status: "authorized", retryAfter: null };
              case "refresh_minecraft_account":
                return { name: "TestPlayer", uuid: "test-profile" };
              case "list_minecraft_versions":
                return {
                  latest: { release: "26.2", snapshot: "24w01a" },
                  versions: [
                    { id: "26.2", versionType: "release", releaseTime: "2026-09-01" },
                    { id: "1.21.1", versionType: "release", releaseTime: "2024-08-08" },
                    { id: "24w01a", versionType: "snapshot", releaseTime: "2024-01-01" },
                    { id: "a1.2.6", versionType: "old_alpha", releaseTime: "2010-12-03" },
                  ],
                };
              case "list_fabric_loader_versions":
                return [{ version: "0.16.0", stable: true }];
              case "list_instance_mods":
                return [];
              case "search_modrinth_mods":
                if (state.delayModSearch)
                  return new Promise((_resolve, reject) => {
                    state.rejectSearch = () => reject(new Error("以前の検索のエラー"));
                  });
                return { hits: [], offset: 0, limit: 20, totalHits: 0 };
              case "minecraft_permission_support":
                return {
                  platform,
                  editable: platform !== "unsupported",
                  audioOutput: platform === "macos" || platform === "linux",
                  desktopIntegration: platform === "macos",
                  graphicsCache: platform === "macos" || platform === "linux",
                  microphone: platform === "macos",
                  clipboard: platform === "macos",
                };
              case "update_minecraft_permissions": {
                if (state.delayPermissions)
                  await new Promise<void>((resolve) => {
                    state.finishPermissions = resolve;
                  });
                if (state.failPermissions) throw new Error("権限を保存できませんでした");
                const item = instances.find((item) => item.id === args.instanceId)!;
                item.permissions = {
                  ...(args as unknown as { permissions: typeof item.permissions }).permissions,
                };
                return { ...item };
              }
              case "rename_minecraft_instance": {
                if (state.delayRename)
                  await new Promise<void>((resolve) => {
                    state.finishRename = resolve;
                  });
                if (state.failRename) throw new Error("保存テストエラー");
                const item = instances.find((item) => item.id === args.instanceId)!;
                item.name = String(args.name);
                return { ...item };
              }
              case "launch_minecraft_instance":
                state.emit("minecraft-status", {
                  instanceId: args.instanceId,
                  status: "running",
                  exitCode: null,
                });
                return 123;
              case "stop_minecraft_instance":
                if (state.failStop) throw new Error("終了テストエラー");
                return;
              case "delete_minecraft_instance":
                instances.splice(
                  instances.findIndex((item) => item.id === args.instanceId),
                  1,
                );
                return;
              case "install_sandbox_instance": {
                // Match the backend contract so UI tests cannot accept an unusable ID.
                if (!/^[a-zA-Z0-9_-]{1,41}$/.test(String(args.instanceId)))
                  throw new Error("Invalid instance ID: expected 1–41 safe characters");
                const item = {
                  ...instances[0],
                  id: String(args.instanceId),
                  name: String(args.name),
                  versionId: String(args.versionId),
                };
                instances.push(item);
                return item;
              }
              default:
                throw new Error(`Unexpected command: ${command}`);
            }
          },
        },
      });
    },
    { count, platform, initialPermissions },
  );
  await page.goto("/");
}
async function openSurvival(page: Page) {
  await page.getByRole("button", { name: "Instances", exact: true }).click();
  await page.getByRole("button", { name: "Survival", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Survival", exact: true })).toBeVisible();
}

test("light/dark, navigation and narrow reflow", async ({ page }) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByLabel("Theme", { exact: true }).selectOption("dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("main")).toHaveCSS("color", "rgb(237, 237, 237)");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  for (const width of [1024, 320]) {
    await page.setViewportSize({ width, height: 640 });
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    ).toBeTruthy();
  }
  await page.getByRole("button", { name: "ナビゲーションを開く" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Instances", exact: true }).click();
  await page.getByRole("button", { name: "Survival", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("button", { name: "Play", exact: true })).toBeVisible();
  await dialog.getByRole("tab", { name: "Overview", exact: true }).focus();
  await page.keyboard.press("End");
  await expect(dialog.getByRole("tab", { name: "Settings", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).toBeTruthy();
});

test("unsaved draft survives failed save and Escape closes one layer", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page.getByRole("tab", { name: "Settings", exact: true }).click();
  await page.getByLabel("表示名", { exact: false }).fill("Edited name");
  await page.keyboard.press("Escape");
  const guard = page.getByRole("dialog", { name: "未保存の変更があります" });
  await expect(guard).toBeVisible();
  await page.evaluate(() => {
    (window as unknown as { __test: { failRename: boolean } }).__test.failRename = true;
  });
  await guard.getByRole("button", { name: "保存して移動" }).click();
  await expect(guard.getByRole("alert")).toContainText("保存テストエラー");
  await page.keyboard.press("Escape");
  await expect(guard).not.toBeVisible();
  await expect(page.getByLabel("表示名", { exact: false })).toHaveValue("Edited name");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "破棄して移動" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("force quit waits for process status and deletion requires exact name", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page
    .getByRole("dialog", { name: "Survival", exact: true })
    .getByRole("button", { name: "Play", exact: true })
    .click();
  await page.getByRole("button", { name: "Force Quit", exact: true }).click();
  const confirm = page.getByRole("dialog", { name: "Minecraftを強制終了しますか？" });
  await expect(confirm.getByRole("button", { name: "Cancel" })).toBeFocused();
  await confirm.getByRole("button", { name: "Force Quit" }).click();
  await expect(page.getByRole("button", { name: "Stopping…" })).toBeDisabled();
  await page.evaluate(() => {
    (
      window as unknown as { __test: { emit: (event: string, payload: unknown) => void } }
    ).__test.emit("minecraft-status", { instanceId: "survival", status: "stopped", exitCode: 0 });
  });
  await expect(
    page
      .getByRole("dialog", { name: "Survival", exact: true })
      .getByRole("button", { name: "Play", exact: true }),
  ).toBeEnabled();
  await page.getByRole("tab", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "削除…", exact: true }).click();
  const deletion = page.getByRole("dialog", { name: "インスタンスを削除しますか？" });
  await expect(deletion.getByRole("button", { name: "完全に削除" })).toBeDisabled();
  await deletion.getByLabel("確認のためインスタンス名を入力").fill("Survival");
  await deletion.getByRole("button", { name: "完全に削除" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Survival", exact: true })).toHaveCount(0);
});

test("creation opens Overview and logs redact credentials", async ({ page }) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "Add instance" }).click();
  await page.getByRole("textbox", { name: "Name", exact: true }).fill("26");
  await expect(page.getByRole("combobox", { name: "Minecraft version", exact: true })).toHaveValue(
    "26.2",
  );
  await page.getByRole("button", { name: "Create", exact: true }).click();
  const created = page.getByRole("dialog", { name: "26", exact: true });
  await expect(created).toBeVisible();
  await expect(created.getByRole("tab", { name: "Overview", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(created).toContainText("26.2");
  await page.keyboard.press("Escape");
  await openSurvival(page);
  await page.getByRole("tab", { name: "Log", exact: true }).click();
  await page.evaluate(() => {
    (
      window as unknown as { __test: { emit: (event: string, payload: unknown) => void } }
    ).__test.emit("minecraft-log", {
      instanceId: "survival",
      stream: "stdout",
      line: "access_token=super-secret Bearer another-secret",
    });
  });
  await expect(page.getByRole("log")).toContainText("[redacted]");
  await expect(page.getByRole("log")).not.toContainText("super-secret");
});

test("large text, OS contrast and reduced motion retain controls", async ({ page }) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "Instances", exact: true }).click();
  await page.screenshot({ path: "test-results/instances-light.png", fullPage: true });
  await page.setViewportSize({ width: 1024, height: 640 });
  await page.evaluate(() => {
    document.documentElement.style.fontSize = "200%";
  });
  await page.emulateMedia({ reducedMotion: "reduce", forcedColors: "active" });
  await page.getByRole("button", { name: "Survival", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Survival", exact: true });
  await expect(dialog.getByRole("button", { name: "Play", exact: true })).toBeInViewport({
    ratio: 1,
  });
  await expect(dialog.getByRole("button", { name: "閉じる", exact: true })).toBeInViewport({
    ratio: 1,
  });
  await expect(dialog.getByRole("button", { name: "Play", exact: true })).toHaveCSS(
    "transition-duration",
    "0s",
  );
  await page.keyboard.press("Escape");
  await page.evaluate(() => {
    document.documentElement.style.fontSize = "";
  });
  await page.emulateMedia({ reducedMotion: "reduce", forcedColors: "none" });
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByLabel("Theme", { exact: true }).selectOption("dark");
  await page.getByRole("button", { name: "Instances", exact: true }).click();
  await page.screenshot({ path: "test-results/instances-dark.png", fullPage: true });
});

test("stop failure preserves Running; Mod catalog remains reachable", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  const instance = page.getByRole("dialog", { name: "Survival", exact: true });
  await instance.getByRole("tab", { name: "Mods", exact: true }).click();
  await instance.getByRole("button", { name: "Add mods" }).click();
  await expect(page.getByRole("dialog", { name: "Add mods", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(instance).toBeVisible();
  await instance.getByRole("button", { name: "Play", exact: true }).click();
  await page.evaluate(() => {
    (window as unknown as { __test: { failStop: boolean } }).__test.failStop = true;
  });
  await instance.getByRole("button", { name: "Force Quit", exact: true }).click();
  await page
    .getByRole("dialog", { name: "Minecraftを強制終了しますか？" })
    .getByRole("button", { name: "Force Quit", exact: true })
    .click();
  await expect(instance.getByRole("alert")).toContainText("終了テストエラー");
  await expect(instance.getByRole("button", { name: "Force Quit", exact: true })).toBeEnabled();
});

test("200 instances scroll locally and search across the list", async ({ page }) => {
  await mockDesktop(page, 200);
  await page.getByRole("button", { name: "Instances", exact: true }).click();
  await expect(page.locator(".instance-row")).toHaveCount(200);
  await page.locator("main").evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.getByRole("button", { name: "Settings", exact: true })).toBeInViewport({
    ratio: 1,
  });
  await page.getByRole("textbox", { name: "インスタンスを検索" }).fill("Instance 199");
  await expect(page.locator(".instance-row")).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Instance 199", exact: true })).toBeVisible();
});

test("authentication controller signs in and out across the account UI", async ({ page }) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  const auth = page.getByRole("dialog", { name: "Microsoft account", exact: true });
  await auth.getByRole("button", { name: "Microsoftでサインイン" }).click();
  await expect(auth.getByText("TestPlayer", { exact: true })).toBeVisible();
  await auth.getByRole("button", { name: "Sign out", exact: true }).click();
  await expect(auth.getByRole("button", { name: "Microsoftでサインイン" })).toBeEnabled();
});

test("instance search and tab memory survive page and dialog navigation", async ({ page }) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "Instances", exact: true }).click();
  await page.getByRole("textbox", { name: "インスタンスを検索" }).fill("Survival");
  await page.getByRole("button", { name: "Home", exact: true }).click();
  await page.getByRole("button", { name: "Instances", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "インスタンスを検索" })).toHaveValue("Survival");
  await page.getByRole("button", { name: "Survival", exact: true }).click();
  await page.getByRole("tab", { name: "Version", exact: true }).click();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Survival", exact: true }).click();
  await expect(page.getByRole("tab", { name: "Version", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("tab", { name: "Java", exact: true }).click();
  await page.getByRole("button", { name: "Home", exact: true }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("tab", { name: "Java", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await page.getByRole("button", { name: "Home", exact: true }).click();
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("tab", { name: "General", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

test("late Mod search results cannot leak into another instance", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page.evaluate(() => {
    (window as unknown as { __test: { delayModSearch: boolean } }).__test.delayModSearch = true;
  });
  await page.getByRole("tab", { name: "Mods", exact: true }).click();
  await page.getByRole("button", { name: "Add mods", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Add mods", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: /^とても長い日本語/ }).click();
  await page.getByRole("tab", { name: "Mods", exact: true }).click();
  await page.evaluate(() => {
    (
      window as unknown as { __test: { rejectSearch: (() => void) | null } }
    ).__test.rejectSearch?.();
  });
  await expect(page.getByText("以前の検索のエラー")).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(1);
});

test("version selector preserves legacy versions when the creation form reopens", async ({
  page,
}) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "Add instance" }).click();
  await page.getByLabel("Releaseのみ").uncheck();
  await page
    .getByRole("combobox", { name: "Minecraft version", exact: true })
    .selectOption("a1.2.6");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Minecraft", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Add instance" }).click();
  await expect(page.getByLabel("Releaseのみ")).not.toBeChecked();
  await expect(page.getByRole("combobox", { name: "Minecraft version", exact: true })).toHaveValue(
    "a1.2.6",
  );
});

test("dialog traps and restores focus, and backdrop dismissal protects drafts", async ({
  page,
}, testInfo) => {
  await mockDesktop(page);
  await openSurvival(page);
  const dialog = page.getByRole("dialog", { name: "Survival", exact: true });
  await expect(dialog.getByRole("heading", { name: "Survival", exact: true })).toBeFocused();
  for (let i = 0; i < 20; i++) {
    await page.keyboard.press(i < 10 ? "Tab" : "Shift+Tab");
    await expect
      .poll(() => dialog.evaluate((element) => element.contains(document.activeElement)))
      .toBe(true);
  }
  await page.getByRole("tab", { name: "Settings", exact: true }).click();
  await page.getByLabel("表示名", { exact: false }).fill("Protected draft");
  const heading = await dialog
    .getByRole("heading", { name: "Survival", exact: true })
    .boundingBox();
  await page.mouse.move(heading!.x + 5, heading!.y + 5);
  await page.mouse.down();
  await page.mouse.move(4, 4);
  await page.mouse.up();
  await expect(page.getByRole("dialog", { name: "未保存の変更があります" })).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await page.mouse.click(4, 4);
  const guard = page.getByRole("dialog", { name: "未保存の変更があります" });
  await expect(guard).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("nested-dialog.png") });
  await page.mouse.click(4, 4);
  await expect(guard).toHaveCount(0);
  await expect(page.getByLabel("表示名", { exact: false })).toHaveValue("Protected draft");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "破棄して移動" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Survival", exact: true })).toBeFocused();
});

test("all tabs activate from the keyboard and label their panel", async ({ page }, testInfo) => {
  await mockDesktop(page);
  await openSurvival(page);
  const dialog = page.getByRole("dialog", { name: "Survival", exact: true });
  const tabs = dialog.getByRole("tab");
  await tabs.first().focus();
  for (let i = 0; i < 11; i++) {
    await expect(tabs.nth(i)).toHaveAttribute("aria-selected", "true");
    await expect(dialog.getByRole("tabpanel")).toHaveAccessibleName(
      (await tabs.nth(i).textContent())!,
    );
    await expect(tabs.nth(i)).toHaveAttribute(
      "aria-controls",
      (await dialog.getByRole("tabpanel").getAttribute("id"))!,
    );
    await page.keyboard.press("ArrowRight");
  }
  await expect(tabs.first()).toBeFocused();
  await page.setViewportSize({ width: 320, height: 640 });
  await page.keyboard.press("End");
  await expect(tabs.last()).toBeInViewport({ ratio: 0.99 });
  await page.screenshot({ path: testInfo.outputPath("narrow-dialog.png") });
  await page.keyboard.press("Home");
  await expect(tabs.first()).toBeInViewport({ ratio: 0.99 });
});

test("tabs track content growth and keep selection visible after resizing without focus", async ({
  page,
}) => {
  await mockDesktop(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const next = page.getByRole("button", { name: "次のタブを表示" });
  await expect(next).toHaveCount(0);
  // Simulate wider translated labels without resizing the tab list itself.
  const java = page.getByRole("tab", { name: "Java", exact: true });
  await java.evaluate((el) => {
    el.style.minWidth = "1000px";
  });
  await expect(next).toBeVisible();
  await java.evaluate((el) => {
    el.style.minWidth = "";
  });
  await expect(next).toHaveCount(0);
  const last = page.getByRole("tab", { name: "Advanced", exact: true });
  await last.click();
  await page.getByRole("tabpanel").focus();
  await page.setViewportSize({ width: 320, height: 640 });
  await expect
    .poll(() =>
      last.evaluate((tab) => {
        const list = tab.closest<HTMLElement>('[role="tablist"]')!;
        const listRect = list.getBoundingClientRect();
        const tabRect = tab.getBoundingClientRect();
        const left = listRect.left + list.clientLeft;
        return tabRect.left >= left - 1 && tabRect.right <= left + list.clientWidth + 1;
      }),
    )
    .toBe(true);
  await expect(page.getByRole("tabpanel")).toBeFocused();
});

test("scroll buttons can reveal both ends without changing the selected tab", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page.setViewportSize({ width: 320, height: 640 });
  const next = page.getByRole("button", { name: "次のタブを表示" });
  await expect(next).toBeVisible();
  const settleScroll = () =>
    page.evaluate(
      () =>
        new Promise<void>((resolve) => {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
        }),
    );
  for (let i = 0; i < 15 && (await next.count()); i++) {
    await next.click();
    await settleScroll();
  }
  await expect(next).toHaveCount(0);
  await expect(page.getByRole("tab", { name: "Settings", exact: true })).toBeInViewport({
    ratio: 0.99,
  });
  const previous = page.getByRole("button", { name: "前のタブを表示" });
  for (let i = 0; i < 15 && (await previous.count()); i++) {
    await previous.click();
    await settleScroll();
  }
  await expect(previous).toHaveCount(0);
  await expect(page.getByRole("tab", { name: "Overview", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

test("progress reports measured values and omits unknown percentages", async ({
  page,
}, testInfo) => {
  await mockDesktop(page);
  await openSurvival(page);
  const dialog = page.getByRole("dialog", { name: "Survival", exact: true });
  for (const total of [100, 0]) {
    await page.evaluate((total) => {
      (
        window as unknown as { __test: { emit: (event: string, payload: unknown) => void } }
      ).__test.emit("minecraft-install-progress", {
        completed: 37,
        total,
        message: "Downloading libraries",
      });
    }, total);
    const progress = dialog.getByRole("progressbar", { name: "Downloading libraries" });
    await expect(progress).toBeVisible();
    if (total) {
      await expect(progress).toHaveAttribute("aria-valuenow", "37");
      await expect(dialog.getByText("37%", { exact: true })).toBeVisible();
      await page.screenshot({ path: testInfo.outputPath("progress-dialog.png") });
    } else {
      await expect(progress).not.toHaveAttribute("aria-valuenow");
      await expect(dialog.getByText("37%", { exact: true })).toHaveCount(0);
    }
  }
});

test("canceling a guarded tab change keeps the draft and active panel", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page.getByRole("tab", { name: "Settings", exact: true }).click();
  await page.getByLabel("表示名", { exact: false }).fill("Keep editing");
  await page.getByRole("tab", { name: "Overview", exact: true }).click();
  const guard = page.getByRole("dialog", { name: "未保存の変更があります" });
  await expect(guard).toBeVisible();
  await guard.getByRole("button", { name: "編集を続ける" }).click();
  await expect(guard).toHaveCount(0);
  await expect(page.getByRole("tab", { name: "Settings", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByLabel("表示名", { exact: false })).toHaveValue("Keep editing");
  await page.getByRole("tab", { name: "Overview", exact: true }).click();
  await page.getByRole("button", { name: "破棄して移動" }).click();
  await expect(page.getByRole("tabpanel")).toHaveAccessibleName("Overview");
});

test("permissions persist per instance and failed saves retain the confirmed value", async ({
  page,
}, testInfo) => {
  await mockDesktop(page, 2, "macos");
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  const game = page.getByRole("switch", { name: "ゲームデータへの書き込み", exact: true });
  const narrator = page.getByRole("switch", { name: "ナレーター", exact: true });
  await expect(game).toBeChecked();
  await expect(narrator).toBeChecked();
  await game.focus();
  await page.keyboard.press("Space");
  await expect(game).not.toBeChecked();
  await expect(narrator).toBeChecked();
  const toast = page.locator(".toast:not([data-ending-style])");
  await expect(toast).toHaveText("権限を保存しました");
  await expect(page.getByRole("tabpanel").getByText("Saved", { exact: true })).toHaveCount(0);
  const viewport = page.viewportSize()!;
  await expect
    .poll(async () => {
      const bounds = (await toast.boundingBox())!;
      return Math.round(bounds.y + bounds.height);
    })
    .toBe(viewport.height - 16);
  const toastBounds = (await toast.boundingBox())!;
  expect(toastBounds.x + toastBounds.width).toBeCloseTo(viewport.width - 16, 0);
  expect(toastBounds.y + toastBounds.height).toBeCloseTo(viewport.height - 16, 0);
  expect(toastBounds.height).toBeLessThanOrEqual(44);
  await expect(toast).toHaveAttribute("data-type", "success");
  await page.screenshot({ path: testInfo.outputPath("permissions-saved-toast.png") });
  await toast.focus();
  await page.getByRole("button", { name: "通知を閉じる", exact: true }).focus();
  await page.keyboard.press("Space");
  await expect(toast).toHaveCount(0);
  await expect(page.getByRole("dialog", { name: "Survival", exact: true })).toBeVisible();
  await page.evaluate(() => {
    (window as any).__test.failPermissions = true;
  });
  await narrator.click();
  await expect(page.getByRole("tabpanel").getByRole("alert")).toContainText(
    "権限を保存できませんでした",
  );
  await expect(narrator).toBeChecked();
  await expect(toast).toHaveAttribute("data-type", "error");
  await expect(toast).toHaveText("保存できませんでした");
  await page.evaluate(() => {
    (window as any).__test.failPermissions = false;
  });
  await page.clock.install();
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(narrator).not.toBeChecked();
  await expect(game).not.toBeChecked();
  await toast.hover();
  await page.clock.fastForward(6000);
  await expect(toast).toBeVisible();
  await page.getByRole("heading", { name: "Survival", exact: true }).hover();
  await page.clock.fastForward(6000);
  await expect(toast).toHaveCount(0);
  await page.clock.resume();
  await page.screenshot({ path: testInfo.outputPath("permissions-macos.png") });
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Survival", exact: true }).click();
  await expect(narrator).not.toBeChecked();
  await expect(game).not.toBeChecked();
  await page.keyboard.press("Escape");
  await page
    .getByRole("button", {
      name: "とても長い日本語のインスタンス名で折り返しと操作ボタンへの到達性を確認する環境",
      exact: true,
    })
    .click();
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  await expect(game).toBeChecked();
  await expect(narrator).toBeChecked();
});

for (const kind of ["Rename", "Permissions"] as const) {
  test(`${kind} saving toast becomes the result and survives dismissal`, async ({
    page,
  }, testInfo) => {
    await mockDesktop(page);
    await openSurvival(page);
    await page
      .getByRole("tab", { name: kind === "Rename" ? "Settings" : "Permissions", exact: true })
      .click();
    await page.evaluate((kind) => {
      (window as any).__test[`delay${kind}`] = true;
    }, kind);
    const save = async () => {
      if (kind === "Rename") {
        await page.getByLabel("表示名", { exact: false }).fill("Renamed instance");
        await page.getByRole("button", { name: "Apply", exact: true }).click();
        await expect(page.getByRole("button", { name: "Apply", exact: true })).toBeDisabled();
      } else {
        await page.getByRole("switch", { name: "ナレーター", exact: true }).click();
      }
    };
    await save();
    const toast = page.locator(".toast:not([data-ending-style])");
    await expect(toast).toHaveText("保存中…");
    await expect(toast).toHaveAttribute("data-type", "neutral");
    const pendingToast = await toast.elementHandle();
    await expect
      .poll(async () => {
        const bounds = (await toast.boundingBox())!;
        return Math.round(bounds.y + bounds.height);
      })
      .toBe(page.viewportSize()!.height - 16);
    await toast.evaluate((element) =>
      Promise.all(element.getAnimations().map((animation) => animation.finished)),
    );
    await page.screenshot({ path: testInfo.outputPath("saving-toast.png") });
    await page.clock.install();
    await page.clock.fastForward(6000);
    await expect(toast).toHaveText("保存中…");
    await page.evaluate((kind) => (window as any).__test[`finish${kind}`](), kind);
    await expect(toast).toHaveAttribute("data-type", "success");
    expect(await pendingToast!.evaluate((element) => element.isConnected)).toBe(true);
    await expect(toast).toHaveText(
      kind === "Rename" ? "表示名を保存しました" : "権限を保存しました",
    );
    await page
      .getByRole("heading", {
        name: kind === "Rename" ? "Renamed instance" : "Survival",
        exact: true,
      })
      .hover();
    await page.clock.fastForward(6000);
    await expect(toast).toHaveCount(0);
    await page.clock.resume();
    await page.evaluate((kind) => {
      (window as any).__test[`fail${kind}`] = true;
    }, kind);
    if (kind === "Rename") {
      await page.getByRole("textbox", { name: "表示名", exact: false }).fill("Failed draft");
      await page.getByRole("button", { name: "Apply", exact: true }).click();
    } else await save();
    await expect(toast).toHaveText("保存中…");
    await toast.hover();
    await toast.getByRole("button", { name: "通知を閉じる" }).click();
    await expect(page.locator(".toast")).toHaveCount(0);
    await page.evaluate((kind) => (window as any).__test[`finish${kind}`](), kind);
    await expect(toast).toHaveText("保存できませんでした");
    await expect(toast).toHaveAttribute("data-type", "error");
    await expect(page.getByRole("tabpanel").getByRole("alert")).toBeVisible();
  });
}

test("permission save continues across tabs and prevents conflicting actions", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  await page.evaluate(() => {
    (window as any).__test.delayPermissions = true;
  });
  await page.getByRole("switch", { name: "ナレーター", exact: true }).click();
  await expect(page.locator(".toast")).toHaveText("保存中…");
  await expect(page.locator(".footer-status")).not.toContainText("保存");
  await expect(page.getByRole("tabpanel").getByText("Saving…", { exact: true })).toHaveCount(0);
  await expect(
    page
      .getByRole("dialog", { name: "Survival", exact: true })
      .getByRole("button", { name: "Play", exact: true }),
  ).toBeDisabled();
  await expect(page.getByRole("switch").first()).toBeDisabled();
  await expect(page.getByRole("switch").last()).toBeDisabled();
  await page.getByRole("tab", { name: "Overview", exact: true }).click();
  await page.keyboard.press("Escape");
  await page.evaluate(() => {
    (window as any).__test.finishPermissions();
  });
  await page.getByRole("button", { name: "Survival", exact: true }).click();
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  await expect(page.getByRole("switch", { name: "ナレーター", exact: true })).not.toBeChecked();
  await page.evaluate(() => {
    (window as any).__test.emit("minecraft-status", {
      instanceId: "survival",
      status: "running",
      exitCode: null,
    });
  });
  await expect(page.getByText("権限を変更するにはゲームを終了してください。")).toBeVisible();
  await expect(page.getByRole("switch").first()).toBeDisabled();
  await expect(page.getByRole("switch").last()).toBeDisabled();
});

test("Linux exposes common permission controls and desktop compatibility limits", async ({
  page,
}) => {
  await mockDesktop(page, 2, "linux");
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  await expect(page.getByText(/LinuxではWayland接続を優先/)).toBeVisible();
  await expect(page.getByText(/Wayland使用時はX11接続を公開しません/)).toBeVisible();
  const narrator = page.getByRole("switch", { name: "ナレーター", exact: true });
  await narrator.click();
  await expect(narrator).not.toBeChecked();
  await expect(page.locator(".toast:not([data-ending-style])")).toHaveText("権限を保存しました");
});

test("unsupported platforms show permissions without offering ineffective edits", async ({
  page,
}) => {
  await mockDesktop(page, 2, "unsupported");
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  await expect(page.getByText("このOSの権限設定はまだ未対応です。")).toBeVisible();
  await expect(page.getByRole("switch").first()).toBeDisabled();
  await expect(page.getByRole("switch").last()).toBeDisabled();
});

test("permission controls reflow in both themes and remain usable with accessibility settings", async ({
  page,
}, testInfo) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  for (const theme of ["light", "dark"]) {
    await page.evaluate((theme) => {
      document.documentElement.dataset.theme = theme;
    }, theme);
    for (const size of [
      { width: 1440, height: 900 },
      { width: 1024, height: 640 },
      { width: 320, height: 640 },
    ]) {
      await page.setViewportSize(size);
      const panel = page.getByRole("tabpanel", { name: "Permissions", exact: true });
      await expect.poll(() => panel.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
      for (const control of await page.getByRole("switch").all()) {
        await control.scrollIntoViewIfNeeded();
        await expect(control).toBeInViewport();
        const bounds = await control.boundingBox();
        expect(bounds!.width).toBeGreaterThanOrEqual(32);
        expect(bounds!.height).toBeGreaterThanOrEqual(32);
      }
      await panel.evaluate((el) => {
        el.scrollTop = 0;
      });
      await page.screenshot({
        path: testInfo.outputPath(`permissions-${theme}-${size.width}.png`),
      });
    }
  }
  await page.emulateMedia({ forcedColors: "active", reducedMotion: "reduce" });
  await page.evaluate(() => {
    document.documentElement.style.fontSize = "200%";
  });
  const narrator = page.getByRole("switch", { name: "ナレーター", exact: true });
  await narrator.focus();
  await page.keyboard.press("Space");
  await expect(narrator).not.toBeChecked();
  await expect(page.getByRole("tab", { name: "Permissions", exact: true })).toBeInViewport();
  await expect
    .poll(() => page.getByRole("tabpanel").evaluate((el) => el.scrollWidth <= el.clientWidth))
    .toBe(true);
  await page.screenshot({ path: testInfo.outputPath("permissions-high-contrast-200.png") });
});

test("toast tones stay compact at the window corner in light and dark themes", async ({
  page,
}, testInfo) => {
  await page.goto("/");
  await page.evaluate(async () => {
    // Vite loads this test-only fixture through its normal module pipeline.
    const path = "/tests/fixtures/toasts.tsx";
    const fixture = await import(path);
    const show = fixture.mountToasts();
    await new Promise(requestAnimationFrame);
    show();
  });
  const toasts = page.locator(".toast");
  await expect(toasts).toHaveCount(4);
  await toasts.first().hover();
  await expect(toasts.first()).toHaveAttribute("data-expanded", "");
  await expect.poll(() => toasts.first().evaluate((el) => el.getAnimations().length)).toBe(0);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((theme) => {
      document.documentElement.dataset.theme = theme;
    }, theme);
    const colors = await toasts.evaluateAll((items) =>
      items.map((item) => {
        const css = getComputedStyle(item);
        return {
          type: item.getAttribute("data-type"),
          text: css.color,
          background: css.backgroundColor,
          height: item.getBoundingClientRect().height,
        };
      }),
    );
    expect(colors.map((item) => item.type).sort()).toEqual([
      "error",
      "neutral",
      "success",
      "warning",
    ]);
    expect(new Set(colors.map((item) => item.text)).size).toBe(4);
    expect(new Set(colors.map((item) => item.background)).size).toBe(4);
    expect(colors.every((item) => item.height <= 44)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath(`toast-tones-${theme}.png`) });
  }
  await page.setViewportSize({ width: 320, height: 640 });
  await expect(page.locator(".toast-viewport")).toBeInViewport({ ratio: 1 });
  await page.screenshot({ path: testInfo.outputPath("toast-tones-narrow.png") });
});

test("toast stack expands, dismisses with motion, and honors reduced motion", async ({
  page,
}, testInfo) => {
  await page.goto("/");
  await page.evaluate(async () => {
    const path = "/tests/fixtures/toasts.tsx";
    const fixture = await import(path);
    const show = fixture.mountToasts(3);
    await new Promise(requestAnimationFrame);
    show();
  });
  const visible = page.locator(".toast:not([data-limited]):not([data-ending-style])");
  await expect(visible).toHaveCount(3);
  await expect
    .poll(() =>
      page.locator(".toast").evaluateAll((els) => els.flatMap((el) => el.getAnimations()).length),
    )
    .toBe(0);
  const stacked = await visible.evaluateAll((els) =>
    els.map((el) => ({
      top: el.getBoundingClientRect().top,
      height: el.getBoundingClientRect().height,
    })),
  );
  expect(stacked[0].top - stacked[1].top).toBeCloseTo(8, 0);
  expect(stacked[1].height / stacked[0].height).toBeCloseTo(0.9, 1);
  await expect(visible.nth(1).locator(".toast-content")).toHaveCSS("opacity", "0");
  await expect(page.locator(".toast[data-limited]")).toHaveCSS("opacity", "0");
  await page.screenshot({ path: testInfo.outputPath("toast-stack-collapsed.png") });
  await visible.first().hover();
  await expect(visible.first()).toHaveAttribute("data-expanded", "");
  await expect
    .poll(() => visible.nth(1).evaluate((el) => el.getAnimations().length))
    .toBeGreaterThan(0);
  await expect
    .poll(() => visible.evaluateAll((els) => els.flatMap((el) => el.getAnimations()).length))
    .toBe(0);
  const expanded = await visible.evaluateAll((els) =>
    els.map((el) => ({
      top: el.getBoundingClientRect().top,
      bottom: el.getBoundingClientRect().bottom,
    })),
  );
  expect(expanded[0].top - expanded[1].bottom).toBeCloseTo(8, 0);
  await expect(visible.nth(1).locator(".toast-content")).toHaveCSS("opacity", "1");
  await page.screenshot({ path: testInfo.outputPath("toast-stack-expanded.png") });
  const front = visible.first();
  const title = await front.textContent();
  await front.getByRole("button", { name: "通知を閉じる", exact: true }).click();
  const ending = page.locator(".toast[data-ending-style]").filter({ hasText: title! });
  await expect(ending).toHaveCount(1);
  await expect.poll(() => ending.evaluate((el) => el.getAnimations().length)).toBeGreaterThan(0);
  await expect(ending).toHaveCount(0);
  await page.getByRole("heading", { name: "Home", exact: true }).hover();
  await expect(visible.first()).not.toHaveAttribute("data-expanded", "");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.getByRole("button", { name: "Home", exact: true }).focus();
  await page.keyboard.press("Tab");
  await visible.first().focus();
  await expect(visible.first()).toHaveAttribute("data-expanded", "");
  await expect(visible.first()).toHaveCSS("transition-duration", "0s");
  await expect
    .poll(() => visible.evaluateAll((els) => els.flatMap((el) => el.getAnimations()).length))
    .toBe(0);
  const countBeforeSwipe = await page.locator(".toast").count();
  const swipeBounds = (await visible.first().boundingBox())!;
  await page.mouse.move(swipeBounds.x + 40, swipeBounds.y + 20);
  await page.mouse.down();
  await page.mouse.move(swipeBounds.x + 240, swipeBounds.y + 20, { steps: 8 });
  await page.mouse.up();
  await expect(page.locator(".toast")).toHaveCount(countBeforeSwipe - 1);
});

for (const platform of ["windows", "macos", "linux"] as const) {
  test(`${platform} granular permissions save independently and expose only supported controls`, async ({
    page,
  }) => {
    await mockDesktop(page, 2, platform);
    await openSurvival(page);
    await page.getByRole("tab", { name: "Permissions", exact: true }).click();
    const skin = page.getByRole("switch", { name: "スキンキャッシュ", exact: true });
    await expect(skin).toBeChecked();
    await skin.click();
    await expect(skin).not.toBeChecked();
    for (const [name, supported] of [
      ["日本語入力・全画面連携", platform === "macos"],
      ["描画キャッシュ", platform !== "windows"],
    ] as const) {
      const control = page.getByRole("switch", { name, exact: true });
      if (supported) {
        await expect(control).toBeChecked();
        await control.click();
        await expect(control).not.toBeChecked();
      } else {
        await expect(control).toHaveCount(0);
      }
    }
    const worlds = page.getByRole("switch", { name: "ワールドの保存", exact: true });
    await worlds.click();
    await expect(worlds).not.toBeChecked();
    await expect(
      page.getByRole("switch", { name: "Modファイルの変更", exact: true }),
    ).toBeChecked();
    const network = page.getByRole("switch", { name: "ネットワーク通信", exact: true });
    await expect(network).not.toBeChecked();
    await network.click();
    await expect(network).toBeChecked();
    await page.getByRole("tab", { name: "Overview", exact: true }).click();
    await page.getByRole("tab", { name: "Permissions", exact: true }).click();
    await expect(worlds).not.toBeChecked();
    await expect(network).toBeChecked();
    await expect(skin).not.toBeChecked();
    await page.getByRole("switch", { name: "ゲームデータへの書き込み", exact: true }).click();
    await expect(worlds).toBeDisabled();
    await expect(network).toBeEnabled();
    const audio = page.getByRole("switch", { name: "通常音声", exact: true });
    const microphone = page.getByRole("switch", { name: "マイク", exact: true });
    const clipboard = page.getByRole("switch", { name: "クリップボード", exact: true });
    if (platform === "windows") {
      await expect(audio).toHaveCount(0);
    } else {
      await expect(audio).toBeChecked();
      await audio.click();
      await expect(audio).not.toBeChecked();
      await expect(page.getByRole("switch", { name: "ナレーター", exact: true })).toBeChecked();
    }
    if (platform === "macos") {
      await expect(microphone).toBeDisabled();
      await audio.click();
      await microphone.click();
      await expect(microphone).toBeChecked();
      await expect(audio).toBeDisabled();
      await clipboard.click();
      await expect(clipboard).toBeChecked();
    } else {
      await expect(microphone).toHaveCount(0);
      await expect(clipboard).toHaveCount(0);
    }
  });
}

test("permissions imported from another OS can be reduced without silently widening access", async ({
  page,
}) => {
  await mockDesktop(page, 2, "windows", { microphone: true, clipboard: true, audioOutput: false });
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  await page.getByRole("button", { name: "マイクの追加許可を解除", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "マイクの追加許可を解除", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "クリップボードの追加許可を解除", exact: true }).click();
  await page.getByRole("button", { name: "通常音声を許可に戻す", exact: true }).click();
  await expect(
    page.getByText("別のOSの設定が残っているため、このままでは起動できません。"),
  ).toHaveCount(0);
});

test("an invalid imported microphone setting can be disabled without enabling audio", async ({
  page,
}) => {
  await mockDesktop(page, 2, "macos", { microphone: true, audioOutput: false });
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  const microphone = page.getByRole("switch", { name: "マイク", exact: true });
  const audio = page.getByRole("switch", { name: "通常音声", exact: true });
  await microphone.click();
  await expect(microphone).not.toBeChecked();
  await expect(microphone).toBeDisabled();
  await expect(audio).not.toBeChecked();
  await expect(audio).toBeEnabled();
});

test("default-enabled compatibility permissions remain readable at narrow widths", async ({
  page,
}, testInfo) => {
  await mockDesktop(page, 2, "macos");
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  for (const width of [1440, 320]) {
    await page.setViewportSize({ width, height: width === 320 ? 640 : 900 });
    const skin = page.getByRole("switch", { name: "スキンキャッシュ", exact: true });
    await skin.focus();
    await expect(skin).toBeChecked();
    await page.screenshot({ path: testInfo.outputPath(`compatibility-${width}.png`) });
    for (const name of ["日本語入力・全画面連携", "描画キャッシュ"]) {
      const control = page.getByRole("switch", { name, exact: true });
      await control.focus();
      await expect(control).toBeChecked();
      await expect(control).toBeInViewport();
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  }
});

async function loadNewsFixture(page: Page) {
  await mockDesktop(page);
  await page.evaluate(() => {
    const state = (window as any).__test;
    state.newsFeed.entries = Array.from({ length: 5 }, (_, i) => ({
      id: `news-${i}`,
      kind: i < 2 ? "javaPatchNotes" : "news",
      title: `Minecraft News ${i + 1}`,
      summary:
        i === 0 ? '<script>alert("untrusted")</script> Minecraft update summary.' : "News summary",
      category: i < 2 ? "Java Patch Notes · snapshot" : "Minecraft: Java Edition",
      date: "2026-09-11T12:32:17.471Z",
      articleUrl: "https://www.minecraft.net/article/test?ref=launcher",
      imageUrl: i === 0 ? "https://launchercontent.mojang.com/images/test.jpg" : null,
    }));
  });
  await page.route("https://launchercontent.mojang.com/images/**", (route) => route.abort());
  await page.getByRole("button", { name: "更新", exact: true }).click();
  await expect(page.locator(".news-row")).toHaveCount(3);
}

test("news shares Home's latest three and opens articles directly in the browser", async ({
  page,
}) => {
  await loadNewsFixture(page);
  await page.getByRole("button", { name: "News", exact: true }).click();
  await expect(page.locator(".news-row")).toHaveCount(5);
  await expect(
    page.getByText('<script>alert("untrusted")</script> Minecraft update summary.', {
      exact: true,
    }),
  ).toBeVisible();
  const article = page.getByRole("button", { name: "Minecraft News 1", exact: true });
  await page.evaluate(() => {
    (window as any).__test.failOpenArticle = true;
  });
  await article.click();
  await expect(page.getByRole("alert")).toContainText("開けませんでした");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__test.failOpenArticle = false;
  });
  await article.focus();
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => (window as any).__test.articleUrl))
    .toBe("https://www.minecraft.net/article/test?ref=launcher");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(article).toBeFocused();
  await page.getByRole("tab", { name: "MonaLauncher", exact: true }).click();
  await expect(page.getByText("MonaLauncherのニュースは未配信です")).toBeVisible();
  await page.getByRole("tab", { name: "Minecraft", exact: true }).click();
  await expect(page.locator(".news-row")).toHaveCount(3);
  await page.getByRole("tab", { name: "Java Patch Notes", exact: true }).click();
  await expect(page.locator(".news-row")).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Minecraft News 1", exact: true })).toBeVisible();
  await expect(
    page.getByText("記事名をクリックすると、既定ブラウザで原文を開きます。"),
  ).toHaveCount(0);
});

test("news keeps cached articles after a failed refresh and supports retry without blocking navigation", async ({
  page,
}) => {
  await loadNewsFixture(page);
  await page.evaluate(() => {
    (window as any).__test.delayNews = true;
  });
  await page.getByRole("button", { name: "更新", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "更新しています" })).toBeVisible();
  await page.getByRole("button", { name: "News", exact: true }).click();
  await expect(page.locator(".news-row")).toHaveCount(5);
  await page.evaluate(() => {
    (window as any).__test.failNews = true;
    (window as any).__test.finishNews();
  });
  await expect(page.getByRole("alert")).toContainText("ニュースの取得に失敗しました");
  await expect(page.getByRole("status").filter({ hasText: "キャッシュ" })).toBeVisible();
  await expect(page.locator(".news-row")).toHaveCount(5);
  await page.evaluate(() => {
    (window as any).__test.failNews = false;
    (window as any).__test.delayNews = false;
    (window as any).__test.newsFeed.entries = [];
  });
  await page.getByRole("button", { name: "再試行" }).click();
  await expect(page.getByText("配信されているニュースはありません。")).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("news rows reflow across themes, narrow widths and accessibility preferences", async ({
  page,
}, testInfo) => {
  await loadNewsFixture(page);
  await page.getByRole("button", { name: "News", exact: true }).click();
  for (const theme of ["light", "dark"]) {
    await page.evaluate((theme) => {
      document.documentElement.dataset.theme = theme;
    }, theme);
    for (const size of [
      { width: 1440, height: 900 },
      { width: 1024, height: 640 },
      { width: 320, height: 640 },
    ]) {
      await page.setViewportSize(size);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      ).toBeTruthy();
      await page.screenshot({ path: testInfo.outputPath(`news-${theme}-${size.width}.png`) });
      await page.getByRole("button", { name: "Minecraft News 1", exact: true }).click();
      await expect(page.getByRole("dialog")).toHaveCount(0);
      await expect
        .poll(() => page.evaluate(() => (window as any).__test.articleUrl))
        .toBe("https://www.minecraft.net/article/test?ref=launcher");
    }
  }
  await page.emulateMedia({ reducedMotion: "reduce", forcedColors: "active" });
  await page.getByRole("tab", { name: "All", exact: true }).focus();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "Java Patch Notes", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await page.getByRole("button", { name: "Minecraft News 1", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("news displays partial refresh warnings and reveals older entries on demand", async ({
  page,
}) => {
  await loadNewsFixture(page);
  await page.evaluate(() => {
    const state = (window as any).__test;
    const template = state.newsFeed.entries[0];
    state.newsFeed.entries = Array.from({ length: 65 }, (_, i) => ({
      ...template,
      id: `patch-${i}`,
      title: `Patch ${i}`,
    }));
    state.newsFeed.cached = true;
    state.newsFeed.warning =
      "Javaパッチノート: 更新に失敗しました。保存済みの記事を表示しています。";
  });
  await page.getByRole("button", { name: "更新", exact: true }).click();
  await page.getByRole("button", { name: "News", exact: true }).click();
  await expect(page.locator(".news-row")).toHaveCount(30);
  await expect(
    page.getByText("Javaパッチノート: 更新に失敗しました。保存済みの記事を表示しています。"),
  ).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "キャッシュ" })).toBeVisible();
  await page.getByRole("button", { name: "もっと表示（30 / 65件）" }).click();
  await expect(page.locator(".news-row")).toHaveCount(60);
  await page.getByRole("button", { name: "もっと表示（60 / 65件）" }).click();
  await expect(page.locator(".news-row")).toHaveCount(65);
  await expect(page.getByRole("button", { name: /もっと表示/ })).toHaveCount(0);
});
