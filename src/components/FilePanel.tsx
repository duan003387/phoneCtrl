import { useCallback, useEffect, useRef, useState } from "react";
import * as files from "../api/files";
import type { FileEntry } from "../types";

interface Props {
  serial: string;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

const HOME = "/sdcard";

function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function FilePanel({ serial, notify }: Props) {
  const [path, setPath] = useState(HOME);
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [pathInput, setPathInput] = useState("");
  const fileInputRef = useRef<HTMLInputElement>(null);

  const load = useCallback(
    async (p: string) => {
      setLoading(true);
      try {
        const list = await files.list(serial, p);
        list.sort((a, b) => Number(b.isDir) - Number(a.isDir) || a.name.localeCompare(b.name));
        setEntries(list);
      } catch (e) {
        setEntries([]);
        notify(String(e), "err");
      } finally {
        setLoading(false);
      }
    },
    [serial, notify],
  );

  useEffect(() => {
    setPathInput(path);
    void load(path);
  }, [path, load]);

  const go = (p: string) => {
    if (p === path) return;
    setPath(p);
  };

  const doMkdir = async () => {
    const name = window.prompt("新文件夹名称");
    if (!name) return;
    try {
      await files.mkdir(serial, `${path.replace(/\/$/, "")}/${name}`);
      notify("已创建", "ok");
      void load(path);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const doRename = async (entry: FileEntry) => {
    const name = window.prompt("新名称", entry.name);
    if (!name || name === entry.name) return;
    try {
      await files.rename(serial, entry.path, `${entry.path.replace(entry.name, "")}${name}`);
      notify("已重命名", "ok");
      void load(path);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const doDelete = async (entry: FileEntry) => {
    if (!window.confirm(`确定删除 ${entry.name} ？`)) return;
    try {
      await files.del(serial, entry.path);
      notify("已删除", "ok");
      void load(path);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const doDownload = async (entry: FileEntry) => {
    try {
      const local = `${entry.name}`;
      await files.download(serial, entry.path, local);
      notify(`已下载到当前目录：${local}`, "ok");
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const onPickFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    e.target.value = "";
    if (!f) return;
    try {
      const buf = new Uint8Array(await f.arrayBuffer());
      let bin = "";
      for (let i = 0; i < buf.length; i += 0x8000) {
        bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
      }
      const b64 = btoa(bin);
      await files.uploadBytes(serial, path, f.name, b64);
      notify(`已上传 ${f.name}`, "ok");
      void load(path);
    } catch (err) {
      notify(String(err), "err");
    }
  };

  const crumbs = path.split("/").filter(Boolean);

  return (
    <div className="panel">
      <div className="toolbar">
        <button className="btn" onClick={() => go("/")} title="根目录">🏠</button>
        <button className="btn" onClick={() => go(path.slice(0, path.lastIndexOf("/")) || "/")} title="上级">↑</button>
        <input
          className="path-input"
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && go(pathInput)}
        />
        <button className="btn ghost" onClick={() => void load(path)}>↻</button>
        <button className="btn" onClick={doMkdir}>新建文件夹</button>
        <button className="btn primary" onClick={() => fileInputRef.current?.click()}>
          上传文件
        </button>
        <input
          ref={fileInputRef}
          type="file"
          style={{ display: "none" }}
          onChange={onPickFile}
        />
      </div>
      <div className="crumbs">
        {path === "/" ? (
          <span className="crumb">/</span>
        ) : (
          <>
            <span className="crumb clickable" onClick={() => go("/")}>/</span>
            {crumbs.map((c, i) => {
              const p = "/" + crumbs.slice(0, i + 1).join("/");
              return (
                <span key={p} className="crumb clickable" onClick={() => go(p)}>
                  {c}/
                </span>
              );
            })}
          </>
        )}
      </div>
      <div className="table-wrap">
        {loading && <div className="loading">加载中…</div>}
        <table className="file-table">
          <thead>
            <tr><th>名称</th><th>大小</th><th>类型</th><th>操作</th></tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={e.path}>
                <td>
                  <span
                    className={e.isDir ? "file-icon dir" : "file-icon"}
                    onClick={() => e.isDir && go(e.path)}
                  >
                    {e.isDir ? "📁 " : "📄 "}
                    <span className={e.isDir ? "name dir" : "name"}>{e.name}</span>
                  </span>
                </td>
                <td>{e.isDir ? "—" : formatSize(e.size)}</td>
                <td>{e.isDir ? "目录" : "文件"}</td>
                <td className="row-actions">
                  {!e.isDir && <button className="btn mini" onClick={() => void doDownload(e)}>下载</button>}
                  <button className="btn mini" onClick={() => void doRename(e)}>重命名</button>
                  <button className="btn mini danger" onClick={() => void doDelete(e)}>删除</button>
                </td>
              </tr>
            ))}
            {!loading && entries.length === 0 && (
              <tr><td colSpan={4} className="empty">目录为空</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
