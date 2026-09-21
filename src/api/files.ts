import { invoke } from "@tauri-apps/api/core";
import type { FileEntry } from "../types";

export const list = (serial: string, path: string) =>
  invoke<FileEntry[]>("files_list", { serial, path });

export const del = (serial: string, path: string) =>
  invoke<void>("files_delete", { serial, path });

export const mkdir = (serial: string, path: string) =>
  invoke<void>("files_mkdir", { serial, path });

export const rename = (serial: string, from: string, to: string) =>
  invoke<void>("files_rename", { serial, from, to });

export const copy = (serial: string, from: string, to: string) =>
  invoke<void>("files_copy", { serial, from, to });

export const upload = (serial: string, local: string, remote: string) =>
  invoke<void>("files_upload", { serial, local, remote });

export const uploadBytes = (serial: string, remoteDir: string, name: string, dataBase64: string) =>
  invoke<void>("files_upload_bytes", { serial, remoteDir, name, data: dataBase64 });

export const download = (serial: string, remote: string, local: string) =>
  invoke<void>("files_download", { serial, remote, local });

export const read = (serial: string, path: string, max?: number) =>
  invoke<string>("files_read", { serial, path, max });
