// 与 Rust 后端序列化契约对齐的类型定义

export interface DeviceInfo {
  serial: string;
  state: string;
  model: string | null;
  product: string | null;
}

export interface DeviceProps {
  serial: string;
  model: string;
  manufacturer: string;
  androidVersion: string;
  sdk: number;
  screenWidth: number;
  screenHeight: number;
  density: number;
  batteryLevel: number;
  awake: boolean;
  rotation: number;
}

export interface AppConfig {
  adbPath: string | null;
  ffmpegPath: string | null;
  streamBitrate: number;
  streamMaxWidth: number;
  streamFps: number;
  streamQuality: number;
  touchBackend: "auto" | "scrcpy";
}

export interface StreamOpts {
  bitrate?: number;
  maxWidth?: number;
  fps?: number;
  quality?: number;
}

export interface StreamMeta {
  serial: string;
  width: number;
  height: number;
  fps: number;
  bitrate: number;
  startedAt: number;
  backend: string;
  streamUrl: string;
}

export interface StreamStateEvent {
  serial: string;
  state: string;
  error: string | null;
}

/** 服务器实际视频尺寸更新（含编码器对齐与旋转变化） */
export interface StreamMetaEvent {
  serial: string;
  width: number;
  height: number;
}

export interface FileEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  permissions: string;
}

export interface AppEntry {
  package: string;
  version: string | null;
  system: boolean;
  enabled: boolean;
}

export interface AppDetail {
  package: string;
  version: string | null;
  uid: string | null;
  system: boolean;
  enabled: boolean;
  label: string | null;
}

export type MacroStep =
  | { type: "tap"; x: number; y: number }
  | { type: "swipe"; x1: number; y1: number; x2: number; y2: number; durationMs: number }
  | { type: "key"; keycode: number }
  | { type: "text"; text: string }
  | { type: "wait"; ms: number }
  | { type: "screenshot" }
  | { type: "home" }
  | { type: "back" }
  | { type: "recents" };

export interface MacroStepAt {
  ts: number;
  step: MacroStep;
}

export interface Macro {
  id: string;
  name: string;
  steps: MacroStepAt[];
  createdAt: number;
  screenSize: [number, number] | null;
}
