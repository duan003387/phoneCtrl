import { useEffect, useState } from "react";
import { diagnostics, settingsGet, settingsSet } from "../api/devices";
import { runUpdateFlow } from "../api/update";
import type { AppConfig } from "../types";
import { IconSettings, IconX } from "./Icons";

interface Props {
  onClose: () => void;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

export function SettingsDialog({ onClose, notify }: Props) {
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [diag, setDiag] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [checkingUpdate, setCheckingUpdate] = useState(false);

  useEffect(() => {
    void settingsGet().then(setCfg);
    void diagnostics().then(setDiag).catch((e) => setDiag(String(e)));
  }, []);

  const save = async () => {
    if (!cfg) return;
    setSaving(true);
    try {
      await settingsSet(cfg);
      notify("配置已成功更新并保存", "ok");
      onClose();
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setSaving(false);
    }
  };

  if (!cfg) return null;

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        {/* 标题栏 */}
        <div className="modal-header">
          <div className="modal-title">
            <IconSettings size={18} style={{ color: "var(--accent)" }} />
            <span>首选项设置</span>
          </div>
          <button className="btn ghost icon-only" onClick={onClose} title="关闭">
            <IconX size={16} />
          </button>
        </div>

        {/* 分区 1：ADB 调试环境 */}
        <div className="settings-section">
          <div className="settings-section-title">ADB 与连接环境</div>
          <div className="form-item">
            <label className="form-label">ADB 命令行路径 (platform-tools)</label>
            <span className="form-desc">默认留空将从系统环境变量 PATH 自动探测</span>
            <input
              className="form-input"
              value={cfg.adbPath ?? ""}
              placeholder="例如：/usr/local/bin/adb 或 C:\platform-tools\adb.exe"
              onChange={(e) =>
                setCfg({ ...cfg, adbPath: e.target.value.trim() || null })
              }
            />
          </div>
        </div>

        {/* 分区 2：投屏流与画质配置 */}
        <div className="settings-section">
          <div className="settings-section-title">投屏性能与画质</div>
          <div className="form-item">
            <label className="form-label">投屏限制最大宽度 (px)</label>
            <span className="form-desc">数值越低延迟越小；默认推荐 1080</span>
            <input
              className="form-input"
              type="number"
              min={360}
              max={1920}
              step={60}
              value={cfg.streamMaxWidth}
              onChange={(e) =>
                setCfg({ ...cfg, streamMaxWidth: Number(e.target.value) })
              }
            />
          </div>

          <div className="form-item">
            <label className="form-label">目标投屏帧率 (FPS)</label>
            <span className="form-desc">基于 WebCodecs 硬解加速，推荐 30~60 帧</span>
            <input
              className="form-input"
              type="number"
              min={5}
              max={60}
              value={cfg.streamFps}
              onChange={(e) =>
                setCfg({ ...cfg, streamFps: Number(e.target.value) })
              }
            />
          </div>

          <div className="form-item">
            <label className="form-label">视频流编码比特率 (bps)</label>
            <span className="form-desc">推荐 4,000,000 ~ 8,000,000 bps</span>
            <input
              className="form-input"
              type="number"
              min={1_000_000}
              max={20_000_000}
              step={500_000}
              value={cfg.streamBitrate}
              onChange={(e) =>
                setCfg({ ...cfg, streamBitrate: Number(e.target.value) })
              }
            />
          </div>
        </div>

        {/* 分区 3：交互与触摸控制后端 */}
        <div className="settings-section">
          <div className="settings-section-title">触摸控制</div>
          <div className="form-item">
            <label className="form-label">触摸输入注入方式</label>
            <select
              className="form-select"
              value={cfg.touchBackend}
              onChange={(e) =>
                setCfg({
                  ...cfg,
                  touchBackend: e.target.value as "auto" | "scrcpy",
                })
              }
            >
              <option value="auto">auto（推荐：基于 adb 兼容性最佳）</option>
              <option value="scrcpy">scrcpy（原生 socket 注入，极低延迟拖拽）</option>
            </select>
          </div>
        </div>

        {/* 环境诊断 */}
        {diag && (
          <div className="settings-section">
            <div className="settings-section-title">运行环境诊断</div>
            <pre className="diag-code-box">{diag}</pre>
          </div>
        )}

        {/* 关于与更新 */}
        <div className="settings-section">
          <div className="settings-section-title">关于与更新</div>
          <div className="form-item">
            <label className="form-label">应用内更新</label>
            <span className="form-desc">检查新版本 → 自动下载 → 安装并重启，无需重新拖入「应用程序」</span>
            <button
              className="btn"
              disabled={checkingUpdate}
              onClick={async () => {
                setCheckingUpdate(true);
                try {
                  await runUpdateFlow(notify, { auto: false });
                } finally {
                  setCheckingUpdate(false);
                }
              }}
            >
              {checkingUpdate ? "检查中…" : "检查更新"}
            </button>
          </div>
        </div>

        {/* 操作区 */}
        <div className="modal-actions">
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button className="btn primary" onClick={() => void save()} disabled={saving}>
            {saving ? "正在保存…" : "保存配置"}
          </button>
        </div>
      </div>
    </div>
  );
}
