import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { streamStart, streamStop } from "../api/stream";
import type { StreamMeta, StreamMetaEvent, StreamStateEvent } from "../types";

export type StreamState = "idle" | "starting" | "streaming" | "error";

export interface StreamSession {
  state: StreamState;
  meta: StreamMeta | null;
  error: string | null;
  start: () => Promise<void>;
  stop: () => Promise<void>;
}

/**
 * 管理投屏流生命周期：启动/停止 + 监听 stream://state 事件。
 * 帧传输走本地 HTTP MJPEG 流（meta.streamUrl），前端 <img> 直接播放。
 */
export function useStream(serial: string | null): StreamSession {
  const [state, setState] = useState<StreamState>("idle");
  const [meta, setMeta] = useState<StreamMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const streamingSerialRef = useRef<string | null>(null);

  const start = useCallback(async () => {
    if (!serial) return;
    streamingSerialRef.current = serial;
    setError(null);
    setState("starting");
    try {
      const m = await streamStart(serial, {});
      setMeta(m);
      setState("streaming");
    } catch (e) {
      setError(String(e));
      setMeta(null);
      setState("error");
      streamingSerialRef.current = null;
    }
  }, [serial]);

  const stopStream = useCallback(async (s: string | null) => {
    if (!s) return;
    if (streamingSerialRef.current === s) {
      streamingSerialRef.current = null;
    }
    setMeta(null);
    setState("idle");
    try {
      await streamStop(s);
    } catch {
      /* 忽略停止时的错误 */
    }
  }, []);

  const stop = useCallback(async () => {
    await stopStream(serial);
  }, [serial, stopStream]);

  // 设备切换/拔出时自动停止旧设备的流
  useEffect(() => {
    const running = streamingSerialRef.current;
    if (running && running !== serial) {
      void stopStream(running);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial, stopStream]);

  // 监听后端流状态事件
  useEffect(() => {
    const un = listen<StreamStateEvent>("stream://state", (e) => {
      const s = e.payload.serial;
      const st = e.payload.state;
      if (st === "error" && streamingSerialRef.current === s) {
        streamingSerialRef.current = null;
        if (s === serial) {
          setState("error");
          setError(e.payload.error ?? "投屏流异常终止");
        }
      }
    });
    return () => {
      un.then((fn) => fn());
    };
  }, [serial]);

  // 监听真实视频尺寸更新（编码器对齐/旋转），坐标映射依赖它
  useEffect(() => {
    const un = listen<StreamMetaEvent>("stream://meta", (e) => {
      if (e.payload.serial !== serial) return;
      setMeta((prev) =>
        prev && prev.serial === e.payload.serial
          ? { ...prev, width: e.payload.width, height: e.payload.height }
          : prev,
      );
    });
    return () => {
      un.then((fn) => fn());
    };
  }, [serial]);

  return { state, meta, error, start, stop };
}
