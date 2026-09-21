import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, DeviceInfo, DeviceProps } from "../types";

export const devicesList = () => invoke<DeviceInfo[]>("devices_list");
export const deviceStartServer = () => invoke<void>("device_start_server");
export const deviceProps = (serial: string) => invoke<DeviceProps>("device_props", { serial });

export const settingsGet = () => invoke<AppConfig>("settings_get");
export const settingsSet = (config: AppConfig) => invoke<void>("settings_set", { config });
export const diagnostics = () => invoke<string>("diagnostics");
