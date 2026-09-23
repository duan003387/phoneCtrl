import { invoke } from "@tauri-apps/api/core";

// 与 Rust automate::Step 对齐的判别联合（serde tag="type"，字段 camelCase）
export type Step =
  | { type: "openApp"; package: string }
  | { type: "tap"; by?: string; value?: string; x?: number; y?: number }
  | { type: "input"; by?: string; value?: string; text: string }
  | { type: "swipe"; direction: string }
  | { type: "wait"; ms: number }
  | { type: "waitFor"; by: string; value: string; timeoutMs?: number }
  | { type: "assertText"; contains: string; timeoutMs?: number }
  | { type: "key"; keycode: number }
  | { type: "screenshot" }
  | { type: "back" }
  | { type: "home" };

export interface StepResult {
  index: number;
  label: string;
  ok: boolean;
  message: string;
  elapsedMs: number;
  screenshot: string | null;
}

export interface RunReport {
  ok: boolean;
  steps: StepResult[];
  elapsedMs: number;
}

export const BY_OPTIONS: { value: string; label: string }[] = [
  { value: "text", label: "文本 (text)" },
  { value: "id", label: "资源ID (id)" },
  { value: "xpath", label: "XPath" },
  { value: "accessibilityId", label: "无障碍ID" },
];

export const run = (serial: string, steps: Step[]) =>
  invoke<RunReport>("automate_run", { serial, steps });

export const env = () =>
  invoke<{ mode: string; appiumReady: boolean }>("automate_env");
