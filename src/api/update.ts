import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { confirmAction } from "../ui/dialogs";

type Notify = (msg: string, kind?: "ok" | "err") => void;

/**
 * 检查 → 提示 → 下载 → 安装 → 重启，全流程。
 * auto=true 时若无更新则静默（不弹"已最新"）。
 */
export async function runUpdateFlow(
  notify: Notify,
  opts: { auto: boolean } = { auto: false }
): Promise<void> {
  try {
    const update = await check();
    if (!update) {
      if (!opts.auto) notify("已是最新版本", "ok");
      return;
    }
    const ok = await confirmAction(
      `发现新版本 v${update.version}${update.body ? `

${update.body}` : ""}

下载完成后会自动安装并重启，是否立即更新？`,
      "发现更新"
    );
    if (!ok) return;

    notify("正在下载更新…", "ok");
    await update.download();
    notify("正在安装并重启…", "ok");
    await update.install();
    await relaunch();
  } catch (e) {
    notify(`更新失败：${e}`, "err");
  }
}
