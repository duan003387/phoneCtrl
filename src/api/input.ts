import { invoke } from "@tauri-apps/api/core";

export const tap = (serial: string, x: number, y: number) =>
  invoke<void>("input_tap", { serial, x, y });

export const swipe = (
  serial: string,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  durationMs: number,
) => invoke<void>("input_swipe", { serial, x1, y1, x2, y2, durationMs });

/** 实时触摸注入（down/move/up），走 scrcpy 控制通道，延迟极低 */
export const touch = (serial: string, action: "down" | "move" | "up", x: number, y: number) =>
  invoke<void>("input_touch", { serial, action, x, y });

export const key = (serial: string, keycode: number) =>
  invoke<void>("input_key", { serial, keycode });

export const text = (serial: string, text: string) =>
  invoke<void>("input_text", { serial, text });
