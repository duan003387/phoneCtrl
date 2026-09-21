import { invoke } from "@tauri-apps/api/core";
import type { AppDetail, AppEntry } from "../types";

export const list = (serial: string, includeSystem: boolean) =>
  invoke<AppEntry[]>("apps_list", { serial, includeSystem });

export const install = (serial: string, localApk: string) =>
  invoke<void>("apps_install", { serial, localApk });

export const installBytes = (serial: string, name: string, dataBase64: string) =>
  invoke<void>("apps_install_bytes", { serial, name, data: dataBase64 });

export const uninstall = (serial: string, packageName: string, keepData: boolean) =>
  invoke<void>("apps_uninstall", { serial, package: packageName, keepData });

export const launch = (serial: string, packageName: string) =>
  invoke<void>("apps_launch", { serial, package: packageName });

export const stop = (serial: string, packageName: string) =>
  invoke<void>("apps_stop", { serial, package: packageName });

export const info = (serial: string, packageName: string) =>
  invoke<AppDetail>("apps_info", { serial, package: packageName });
