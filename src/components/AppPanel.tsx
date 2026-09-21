import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as apps from "../api/apps";
import { confirmDanger } from "../ui/dialogs";
import type { AppEntry } from "../types";
import {
  IconApps,
  IconUpload,
  IconRefresh,
  IconSearch,
  IconPlay,
  IconStop,
  IconTrash,
} from "./Icons";

interface Props {
  serial: string;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

export function AppPanel({ serial, notify }: Props) {
  const [includeSystem, setIncludeSystem] = useState(false);
  const [appsList, setAppsList] = useState<AppEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
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

  const filteredApps = useMemo(() => {
    if (!searchQuery.trim()) return appsList;
    const q = searchQuery.toLowerCase();
    return appsList.filter((a) => a.package.toLowerCase().includes(q));
  }, [appsList, searchQuery]);

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
      notify(`已成功安装应用：${f.name}`, "ok");
      void load();
    } catch (err) {
      notify(String(err), "err");
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="panel">
      {/* 顶部操作与搜索 */}
      <div className="panel-toolbar">
        <div className="toolbar-left">
          <button
            className="btn primary"
            onClick={() => fileInputRef.current?.click()}
            disabled={busy !== null}
          >
            <IconUpload size={14} />
            <span>{busy === "install" ? "正在安装…" : "安装 APK"}</span>
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".apk"
            style={{ display: "none" }}
            onChange={doInstall}
          />

          <div className="search-box">
            <IconSearch size={14} style={{ color: "var(--text-tertiary)" }} />
            <input
              className="search-input"
              placeholder="搜索应用包名…"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
          </div>

          <label className="switch-label">
            <input
              type="checkbox"
              className="switch-input"
              checked={includeSystem}
              onChange={(e) => setIncludeSystem(e.target.checked)}
            />
            <span>显示系统应用</span>
          </label>
        </div>

        <div className="toolbar-right">
          <button className="btn ghost icon-only" onClick={() => void load()} title="刷新应用列表">
            <IconRefresh size={14} />
          </button>
          <span className="badge">
            {loading ? "正在获取…" : `共 ${filteredApps.length} 个应用`}
          </span>
        </div>
      </div>

      {/* 应用列表 */}
      <div className="table-wrap">
        <table className="modern-table">
          <thead>
            <tr>
              <th style={{ width: "65%" }}>应用包名 (Package Name)</th>
              <th style={{ width: "35%", textAlign: "right" }}>操作控制</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={2} className="loading-state">
                  正在加载已安装应用…
                </td>
              </tr>
            ) : filteredApps.length === 0 ? (
              <tr>
                <td colSpan={2} className="empty-cell">
                  {searchQuery ? "未匹配到相关应用" : "暂无已安装应用"}
                </td>
              </tr>
            ) : (
              filteredApps.map((a) => {
                const isCurrentBusy = busy === a.package;
                return (
                  <tr key={a.package}>
                    <td>
                      <div className="file-item-name">
                        <span className="file-icon-badge">
                          <IconApps size={14} />
                        </span>
                        <span className="mono" style={{ fontSize: 13 }}>
                          {a.package}
                        </span>
                      </div>
                    </td>
                    <td>
                      <div className="row-actions" style={{ justifyContent: "flex-end" }}>
                        <button
                          className="btn mini"
                          disabled={busy !== null}
                          onClick={() =>
                            void run(
                              a.package,
                              () => apps.launch(serial, a.package),
                              `已启动：${a.package}`
                            )
                          }
                          title="在手机上前台启动该应用"
                        >
                          <IconPlay size={12} />
                          <span>{isCurrentBusy ? "处理中…" : "启动"}</span>
                        </button>
                        <button
                          className="btn mini"
                          disabled={busy !== null}
                          onClick={() =>
                            void run(
                              a.package,
                              () => apps.stop(serial, a.package),
                              `已强制停止：${a.package}`
                            )
                          }
                          title="强制结束应用进程"
                        >
                          <IconStop size={12} />
                          <span>停止</span>
                        </button>
                        <button
                          className="btn mini danger"
                          disabled={busy !== null}
                          onClick={async () => {
                            if (!(await confirmDanger(`确定卸载「${a.package}」？`, "确认卸载"))) return;
                            void run(
                              a.package,
                              () => apps.uninstall(serial, a.package, false),
                              `已卸载：${a.package}`
                            );
                          }}
                          title="卸载该应用"
                        >
                          <IconTrash size={12} />
                          <span>卸载</span>
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
