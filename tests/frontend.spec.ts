import { test, expect, type Page } from "@playwright/test";

async function mockDesktop(
  page: Page,
  count = 2,
  platform: "windows" | "macos" | "unsupported" = "windows",
) {
  await page.addInitScript(
    ({ count, platform }) => {
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
          permissions: { gameWrite: true, narrator: true },
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
          permissions: { gameWrite: true, narrator: true },
          demo: false,
          modLoader: { type: "vanilla" },
        },
      ];
      for (let i = 2; i < count; i++)
        instances.push({ ...instances[0], id: `fixture-${i}`, name: `Instance ${i}` });
      const state = {
        calls: [] as string[],
        failRename: false,
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
                return { platform, editable: platform !== "unsupported" };
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
    { count, platform },
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
  await page.evaluate(() => {
    (window as any).__test.failPermissions = true;
  });
  await narrator.click();
  await expect(page.getByRole("alert")).toContainText("権限を保存できませんでした");
  await expect(narrator).toBeChecked();
  await expect(page.getByText("Saved", { exact: true })).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__test.failPermissions = false;
  });
  await page.getByRole("button", { name: "再試行", exact: true }).click();
  await expect(narrator).not.toBeChecked();
  await expect(game).not.toBeChecked();
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

test("permission save continues across tabs and prevents conflicting actions", async ({ page }) => {
  await mockDesktop(page);
  await openSurvival(page);
  await page.getByRole("tab", { name: "Permissions", exact: true }).click();
  await page.evaluate(() => {
    (window as any).__test.delayPermissions = true;
  });
  await page.getByRole("switch", { name: "ナレーター", exact: true }).click();
  await expect(page.getByText("Saving…", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("dialog").getByText("権限を保存しています…", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("dialog").getByRole("button", { name: "Play", exact: true }),
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
