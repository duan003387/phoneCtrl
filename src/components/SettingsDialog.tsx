import { useEffect, useState } from "react";
import { diagnostics, settingsGet, settingsSet } from "../api/devices";
import type { AppConfig } from "../types";

interface Props {
  onClose: () => void;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

export function SettingsDialog({ onClose, notify }: Props) {
  const [cfg, setCfg] = useState<AppConfig | null>(null);
  const [diag, setDiag] = useState<string>("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void settingsGet().then(setCfg);
    void diagnostics().then(setDiag).catch((e) => setDiag(String(e)));
  }, []);

  const save = async () => {
    if (!cfg) return;
    setSaving(true);
    try {
      await settingsSet(cfg);
      notify("设置已保存", "ok");
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
        <h3>设置</h3>
        <label className="field">
          adb 路径（Android SDK platform-tools）
          <input value={cfg.adbPath ?? ""} placeholder="留空自动探测"
            onChange={(e) => setCfg({ ...cfg, adbPath: e.target.value || null })} />
        </label>
        <label className="field">
          投屏最大宽度（px，越小越流畅）
          <input type="number" min={360} max={1920} value={cfg.streamMaxWidth}
            onChange={(e) => setCfg({ ...cfg, streamMaxWidth: Number(e.target.value) })} />
        </label>
        <label className="field">
          投屏帧率（fps，H.264 + WebCodecs 硬解，可至 60）
          <input type="number" min={5} max={60} value={cfg.streamFps}
            onChange={(e) => setCfg({ ...cfg, streamFps: Number(e.target.value) })} />
        </label>
        <label className="field">
          比特率（bps，越高越清晰）
          <input type="number" min={1_000_000} max={20_000_000} step={500_000} value={cfg.streamBitrate}
            onChange={(e) => setCfg({ ...cfg, streamBitrate: Number(e.target.value) })} />
        </label>
        {diag && (
          <pre className="diag">环境诊断：{diag}</pre>
        )}
        <div className="modal-actions">
          <button className="btn" onClick={onClose}>取消</button>
          <button className="btn primary" onClick={() => void save()} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
