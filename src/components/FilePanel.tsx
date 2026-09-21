import { useCallback, useEffect, useRef, useState } from "react";
import * as files from "../api/files";
import { promptText, confirmDanger } from "../ui/dialogs";
import type { FileEntry } from "../types";
import {
  IconFolder,
  IconFile,
  IconRefresh,
  IconFolderPlus,
  IconUpload,
  IconDownload,
  IconEdit,
  IconTrash,
  IconHome,
  IconChevronUp,
} from "./Icons";

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
  const [isEditingPath, setIsEditingPath] = useState(false);
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
    setIsEditingPath(false);
  };

  const doMkdir = async () => {
    const name = await promptText("新文件夹", "", "文件夹名称");
    if (!name) return;
    try {
      await files.mkdir(serial, `${path.replace(/\/$/, "")}/${name}`);
      notify("文件夹已创建", "ok");
      void load(path);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const doRename = async (entry: FileEntry) => {
    const name = await promptText("新名称", entry.name, "新名称");
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
    if (!(await confirmDanger(`确定删除「${entry.name}」？`))) return;
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
      notify(`已下载至本地目录：${local}`, "ok");
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
      notify(`已成功上传：${f.name}`, "ok");
      void load(path);
    } catch (err) {
      notify(String(err), "err");
    }
  };

  const crumbs = path.split("/").filter(Boolean);

  return (
    <div className="panel">
      {/* 顶部工具栏与路径 */}
      <div className="panel-toolbar">
        <div className="toolbar-left" style={{ flex: 1 }}>
          <button className="btn ghost icon-only" onClick={() => go("/")} title="根目录">
            <IconHome size={15} />
          </button>
          <button
            className="btn ghost icon-only"
            onClick={() => go(path.slice(0, path.lastIndexOf("/")) || "/")}
            title="返回上级目录"
          >
            <IconChevronUp size={15} />
          </button>

          <div className="address-bar">
            {isEditingPath ? (
              <input
                autoFocus
                className="address-input"
                value={pathInput}
                onChange={(e) => setPathInput(e.target.value)}
                onBlur={() => setIsEditingPath(false)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") go(pathInput);
                  if (e.key === "Escape") setIsEditingPath(false);
                }}
              />
            ) : (
              <div className="crumbs-pills" onClick={() => setIsEditingPath(true)} title="点击直接编辑路径">
                <span className="crumb-pill" onClick={(e) => { e.stopPropagation(); go("/"); }}>
                  /
                </span>
                {crumbs.map((c, i) => {
                  const p = "/" + crumbs.slice(0, i + 1).join("/");
                  return (
                    <span key={p} style={{ display: "inline-flex", alignItems: "center", gap: 3 }}>
                      <span className="crumb-sep">/</span>
                      <span
                        className="crumb-pill"
                        onClick={(e) => {
                          e.stopPropagation();
                          go(p);
                        }}
                      >
                        {c}
                      </span>
                    </span>
                  );
                })}
              </div>
            )}
          </div>

          <button className="btn ghost icon-only" onClick={() => void load(path)} title="刷新目录">
            <IconRefresh size={14} />
          </button>
        </div>

        <div className="toolbar-right">
          <button className="btn" onClick={doMkdir}>
            <IconFolderPlus size={15} />
            <span>新建文件夹</span>
          </button>
          <button className="btn primary" onClick={() => fileInputRef.current?.click()}>
            <IconUpload size={15} />
            <span>上传文件</span>
          </button>
          <input
            ref={fileInputRef}
            type="file"
            style={{ display: "none" }}
            onChange={onPickFile}
          />
        </div>
      </div>

      {/* 文件列表表格 */}
      <div className="table-wrap">
        <table className="modern-table">
          <thead>
            <tr>
              <th style={{ width: "45%" }}>名称</th>
              <th style={{ width: "15%" }}>大小</th>
              <th style={{ width: "15%" }}>类型</th>
              <th style={{ width: "25%", textAlign: "right" }}>操作</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={4} className="loading-state">
                  正在加载文件列表…
                </td>
              </tr>
            ) : entries.length === 0 ? (
              <tr>
                <td colSpan={4} className="empty-cell">
                  该目录下没有任何文件或文件夹
                </td>
              </tr>
            ) : (
              entries.map((e) => {
                const isApk = e.name.toLowerCase().endsWith(".apk");
                return (
                  <tr key={e.path}>
                    <td>
                      <div
                        className={`file-item-name ${e.isDir ? "clickable" : ""}`}
                        onClick={() => e.isDir && go(e.path)}
                      >
                        <span className={`file-icon-badge ${e.isDir ? "dir" : isApk ? "apk" : ""}`}>
                          {e.isDir ? (
                            <IconFolder size={15} />
                          ) : (
                            <IconFile size={15} />
                          )}
                        </span>
                        <span className="name-text">{e.name}</span>
                      </div>
                    </td>
                    <td className="mono">{e.isDir ? "—" : formatSize(e.size)}</td>
                    <td>
                      <span className={`badge ${e.isDir ? "info" : ""}`}>
                        {e.isDir ? "文件夹" : isApk ? "APK" : "文件"}
                      </span>
                    </td>
                    <td>
                      <div className="row-actions" style={{ justifyContent: "flex-end" }}>
                        {!e.isDir && (
                          <button className="btn mini" onClick={() => void doDownload(e)} title="下载至本地">
                            <IconDownload size={12} />
                            <span>下载</span>
                          </button>
                        )}
                        <button className="btn mini" onClick={() => void doRename(e)} title="重命名">
                          <IconEdit size={12} />
                          <span>改名</span>
                        </button>
                        <button className="btn mini danger" onClick={() => void doDelete(e)} title="删除文件">
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
