import { invoke } from "@tauri-apps/api/core";

// 通用按键动作（Home/Back/Recents/音量/电源等，keycode 见 input.ts 常量）
export const key = (serial: string, keycode: number) =>
  invoke<void>("action_key", { serial, keycode });

export const screenshot = (serial: string) =>
  invoke<string>("action_screenshot", { serial });

export const screenshotSave = (serial: string) =>
  invoke<string>("action_screenshot_save", { serial });

export const record = (serial: string, seconds: number) =>
  invoke<string>("action_record", { serial, seconds });

/** 保存前端录制的镜像视频（base64）到本地，返回路径。 */
export const saveLocalVideo = (name: string, data: string) =>
  invoke<string>("save_local_video", { name, data });
