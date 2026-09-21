import { useCallback, useEffect, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import * as macrosApi from "../api/macros";
import { confirmDanger } from "../ui/dialogs";
import type { Macro } from "../types";
import { IconMacro, IconPlay, IconTrash, IconRefresh } from "./Icons";

interface Props {
  serial: string | null;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

export function MacroPanel({ serial, notify }: Props) {
  const [macros, setMacros] = useState<Macro[]>([]);
  const [playingId, setPlayingId] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);

  const load = useCallback(async () => {
    try {
      setMacros(await macrosApi.list());
    } catch (e) {
      notify(String(e), "err");
    }
  }, [notify]);

  useEffect(() => {
    void load();
  }, [load]);

  const play = async (m: Macro) => {
    if (!serial) {
      notify("请先在顶栏选择目标设备", "err");
      return;
    }
    setPlayingId(m.id);
    setProgress(0);
    const ch = new Channel<number>();
    ch.onmessage = (n) => setProgress(n);
    try {
      await macrosApi.play(serial, m.id, ch);
      notify(`宏「${m.name}」回放执行完成`, "ok");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setPlayingId(null);
    }
  };

  const del = async (m: Macro) => {
    if (!(await confirmDanger(`确定删除宏「${m.name}」？`))) return;
    try {
      await macrosApi.remove(m.id);
      notify(`已删除宏「${m.name}」`, "ok");
      void load();
    } catch (e) {
      notify(String(e), "err");
    }
  };

  return (
    <div className="panel">
      {/* 顶部工具栏 */}
      <div className="panel-toolbar">
        <div className="toolbar-left">
          <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
            在「镜像」投屏页可点击“录制宏”自动记录键鼠触控操作
          </span>
        </div>
        <div className="toolbar-right">
          <button className="btn ghost icon-only" onClick={() => void load()} title="刷新宏列表">
            <IconRefresh size={14} />
          </button>
          <span className="badge">已保存 {macros.length} 个动作序列</span>
        </div>
      </div>

      {/* 宏列表 */}
      <div className="table-wrap">
        <table className="modern-table">
          <thead>
            <tr>
              <th style={{ width: "35%" }}>宏名称</th>
              <th style={{ width: "20%" }}>动作步数</th>
              <th style={{ width: "25%" }}>录制时间</th>
              <th style={{ width: "20%", textAlign: "right" }}>操作</th>
            </tr>
          </thead>
          <tbody>
            {macros.length === 0 ? (
              <tr>
                <td colSpan={4} className="empty-cell">
                  暂无宏脚本，请前往「镜像」标签页录制动作
                </td>
              </tr>
            ) : (
              macros.map((m) => {
                const isPlaying = playingId === m.id;
                const percent = m.steps.length > 0 ? Math.round((progress / m.steps.length) * 100) : 0;
                return (
                  <tr key={m.id}>
                    <td>
                      <div className="file-item-name">
                        <span className="file-icon-badge">
                          <IconMacro size={14} />
                        </span>
                        <div>
                          <div style={{ fontWeight: 600 }}>{m.name}</div>
                          {isPlaying && (
                            <div className="progress-bar-wrap" style={{ width: 140 }}>
                              <div
                                className="progress-bar-inner"
                                style={{ width: `${percent}%` }}
                              />
                            </div>
                          )}
                        </div>
                      </div>
                    </td>
                    <td>
                      <span className="badge info">{m.steps.length} 步动作</span>
                    </td>
                    <td className="mono" style={{ color: "var(--text-secondary)" }}>
                      {new Date(m.createdAt).toLocaleString()}
                    </td>
                    <td>
                      <div className="row-actions" style={{ justifyContent: "flex-end" }}>
                        <button
                          className={`btn mini ${isPlaying ? "warn" : "primary"}`}
                          disabled={playingId !== null && !isPlaying}
                          onClick={() => void play(m)}
                          title="在手机上依次回放该动作序列"
                        >
                          <IconPlay size={12} />
                          <span>
                            {isPlaying ? `回放中 (${progress}/${m.steps.length})` : "回放"}
                          </span>
                        </button>
                        <button
                          className="btn mini danger"
                          disabled={playingId !== null}
                          onClick={() => void del(m)}
                          title="删除该宏记录"
                        >
                          <IconTrash size={12} />
                          <span>删除</span>
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
