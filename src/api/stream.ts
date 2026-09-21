import { invoke } from "@tauri-apps/api/core";
import type { StreamMeta, StreamOpts } from "../types";

export function streamStart(serial: string, opts: StreamOpts): Promise<StreamMeta> {
  return invoke("stream_start", { serial, opts });
}

export const streamStop = (serial: string) => invoke<void>("stream_stop", { serial });
export const streamStatus = (serial: string) => invoke<StreamMeta | null>("stream_status", { serial });
