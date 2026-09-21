import { useCallback, useEffect, useState } from "react";
import { Channel } from "@tauri-apps/api/core";
import * as macrosApi from "../api/macros";
import type { Macro } from "../types";

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
      notify("请先选择设备", "err");
      return;
    }
    setPlayingId(m.id);
    setProgress(0);
    const ch = new Channel<number>();
    ch.onmessage = (n) => setProgress(n);
    try {
      await macrosApi.play(serial, m.id, ch);
      notify(`宏「${m.name}」回放完成`, "ok");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setPlayingId(null);
    }
  };

  const del = async (m: Macro) => {
    if (!window.confirm(`删除宏「${m.name}」？`)) return;
    try {
      await macrosApi.remove(m.id);
      void load();
    } catch (e) {
      notify(String(e), "err");
    }
  };

  return (
    <div className="panel">
      <div className="toolbar">
        <button className="btn ghost" onClick={() => void load()}>↻</button>
        <span className="toolbar-status">在「镜像」页可录制宏，共 {macros.length} 个</span>
      </div>
      <div className="table-wrap">
        <table className="file-table">
          <thead>
            <tr><th>名称</th><th>动作数</th><th>录制时间</th><th>操作</th></tr>
          </thead>
          <tbody>
            {macros.map((m) => (
              <tr key={m.id}>
                <td>{m.name}</td>
                <td>{m.steps.length}</td>
                <td>{new Date(m.createdAt).toLocaleString()}</td>
                <td className="row-actions">
                  <button className="btn mini primary" disabled={playingId !== null} onClick={() => void play(m)}>
                    {playingId === m.id ? `回放中 ${progress}/${m.steps.length}` : "回放"}
                  </button>
                  <button className="btn mini danger" disabled={playingId !== null} onClick={() => void del(m)}>
                    删除
                  </button>
                </td>
              </tr>
            ))}
            {macros.length === 0 && (
              <tr><td colSpan={4} className="empty">暂无宏，先在「镜像」页录制一个吧</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
