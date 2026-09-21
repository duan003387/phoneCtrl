import { useCallback, useEffect, useRef, useState } from "react";
import * as apps from "../api/apps";
import type { AppEntry } from "../types";

interface Props {
  serial: string;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

export function AppPanel({ serial, notify }: Props) {
  const [includeSystem, setIncludeSystem] = useState(false);
  const [appsList, setAppsList] = useState<AppEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await apps.list(serial, includeSystem);
      setAppsList(list);
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setLoading(false);
    }
  }, [serial, includeSystem, notify]);

  useEffect(() => {
    void load();
  }, [load]);

  const run = async (pkg: string, op: () => Promise<unknown>, okMsg: string) => {
    setBusy(pkg);
    try {
      await op();
      notify(okMsg, "ok");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setBusy(null);
    }
  };

  const doInstall = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    e.target.value = "";
    if (!f) return;
    setBusy("install");
    try {
      const buf = new Uint8Array(await f.arrayBuffer());
      let bin = "";
      for (let i = 0; i < buf.length; i += 0x8000) {
        bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
      }
      await apps.installBytes(serial, f.name, btoa(bin));
      notify(`已安装 ${f.name}`, "ok");
      void load();
    } catch (err) {
      notify(String(err), "err");
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="panel">
      <div className="toolbar">
        <button className="btn primary" onClick={() => fileInputRef.current?.click()} disabled={busy !== null}>
          {busy === "install" ? "安装中…" : "安装 APK"}
        </button>
        <input ref={fileInputRef} type="file" accept=".apk" style={{ display: "none" }} onChange={doInstall} />
        <label className="check">
          <input type="checkbox" checked={includeSystem} onChange={(e) => setIncludeSystem(e.target.checked)} />
          显示系统应用
        </label>
        <button className="btn ghost" onClick={() => void load()}>↻</button>
        <span className="toolbar-status">{loading ? "加载中…" : `${appsList.length} 个应用`}</span>
      </div>
      <div className="table-wrap">
        <table className="file-table">
          <thead>
            <tr><th>包名</th><th>操作</th></tr>
          </thead>
          <tbody>
            {appsList.map((a) => (
              <tr key={a.package}>
                <td className="mono">{a.package}</td>
                <td className="row-actions">
                  <button className="btn mini" disabled={busy !== null} onClick={() => void run(a.package, () => apps.launch(serial, a.package), "已启动")}>
                    启动
                  </button>
                  <button className="btn mini" disabled={busy !== null} onClick={() => void run(a.package, () => apps.stop(serial, a.package), "已停止")}>
                    停止
                  </button>
                  <button
                    className="btn mini danger"
                    disabled={busy !== null}
                    onClick={() => {
                      if (!window.confirm(`卸载 ${a.package} ？`)) return;
                      void run(a.package, () => apps.uninstall(serial, a.package, false), "已卸载");
                    }}
                  >
                    卸载
                  </button>
                </td>
              </tr>
            ))}
            {!loading && appsList.length === 0 && (
              <tr><td colSpan={2} className="empty">没有应用</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
