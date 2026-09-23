import { useEffect, useRef, useState } from "react";
import { key as actionKey, screenshotSave, saveLocalVideo } from "../api/actions";
import { text } from "../api/input";
import {
  KEYCODE_POWER,
  KEYCODE_WAKEUP,
  KEYCODE_VOLUME_UP,
  KEYCODE_VOLUME_DOWN,
} from "../keycodes";
import type { StreamSession } from "../hooks/useStream";
import type { MacroRecorder } from "../hooks/useMacroRecorder";
import {
  IconPower,
  IconSun,
  IconVolumeUp,
  IconVolumeDown,
  IconCamera,
  IconVideo,
  IconSend,
} from "./Icons";

interface Props {
  serial: string;
  stream: StreamSession;
  recorder: MacroRecorder;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

// ArrayBuffer → base64（分块避免超长参数栈溢出）。
async function abToB64(buf: ArrayBuffer): Promise<string> {
  const bytes = new Uint8Array(buf);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode.apply(
      null,
      Array.from(bytes.subarray(i, i + chunk))
    );
  }
  return btoa(binary);
}

export function ControlOverlay({ serial, stream, recorder, notify }: Props) {
  const [inputText, setInputText] = useState("");
  const [recordingScreen, setRecordingScreen] = useState(false);
  const [recElapsed, setRecElapsed] = useState(0);
  const recRef = useRef<MediaRecorder | null>(null);

  // 录制时长计时
  useEffect(() => {
    if (!recordingScreen) return;
    const t0 = Date.now();
    setRecElapsed(0);
    const id = setInterval(() => setRecElapsed(Math.floor((Date.now() - t0) / 1000)), 500);
    return () => clearInterval(id);
  }, [recordingScreen]);

  // 卸载时若仍在录制，停止并释放，避免泄漏
  useEffect(() => {
    return () => {
      try {
        if (recRef.current && recRef.current.state !== "inactive") recRef.current.stop();
      } catch {
        /* noop */
      }
      recRef.current = null;
    };
  }, []);

  const sendKey = (keycode: number) => {
    void actionKey(serial, keycode);
    recorder.push({ type: "key", keycode });
  };

  const doScreenshot = async () => {
    try {
      const path = await screenshotSave(serial);
      notify(`截屏已保存：${path}`, "ok");
    } catch (e) {
      notify(String(e), "err");
    }
  };

  // 录屏：点一次开始、再点停止并保存（捕获镜像 canvas，不依赖设备端 screenrecord）。
  const doRecord = () => {
    // 已在录制 → 停止（onstop 里保存并复位状态）
    if (recordingScreen && recRef.current) {
      try {
        if (recRef.current.state !== "inactive") recRef.current.stop();
      } catch (e) {
        notify(`停止录屏失败：${e}`, "err");
      }
      recRef.current = null;
      return;
    }
    // 开始录制
    const canvas = document.querySelector(
      "canvas.mirror-img"
    ) as HTMLCanvasElement | null;
    if (!canvas || !stream.meta) {
      notify("请先开始投屏，再录屏（录制的是镜像画面）", "err");
      return;
    }
    let rec: MediaRecorder;
    try {
      const s = canvas.captureStream(30);
      const mime = [
        "video/mp4;codecs=avc1.42E01E",
        "video/mp4",
        "video/webm;codecs=vp9",
        "video/webm;codecs=vp8",
        "video/webm",
      ].find((t) => MediaRecorder.isTypeSupported(t)) || "";
      rec = new MediaRecorder(
        s,
        mime ? { mimeType: mime, videoBitsPerSecond: 8_000_000 } : undefined
      );
    } catch (e) {
      notify(`录屏启动失败：${e}`, "err");
      return;
    }
    const usedMime = rec.mimeType || "video/webm";
    const ext = usedMime.includes("mp4") ? "mp4" : "webm";
    const chunks: BlobPart[] = [];
    rec.ondataavailable = (e) => {
      if (e.data.size > 0) chunks.push(e.data);
    };
    rec.onstop = async () => {
      try {
        const blob = new Blob(chunks, { type: usedMime });
        const buf = await blob.arrayBuffer();
        const b64 = await abToB64(buf);
        const path = await saveLocalVideo(`phonectrl_rec_${Date.now()}.${ext}`, b64);
        notify(`录屏已保存：${path}`, "ok");
      } catch (e) {
        notify(String(e), "err");
      } finally {
        setRecordingScreen(false);
      }
    };
    rec.start();
    recRef.current = rec;
    setRecordingScreen(true);
    notify("录制中，再次点击按钮停止并保存", "ok");
  };

  const sendText = () => {
    const t = inputText.trim();
    if (!t) return;
    void text(serial, t);
    recorder.push({ type: "text", text: t });
    setInputText("");
  };

  return (
    <div className="sidebar-control-panel">
      <div className="sidebar-section-title">手机控制</div>

      {/* 硬件物理按键 */}
      <div className="sidebar-key-grid-4">
        <button className="sidebar-action-btn mini" title="电源键" onClick={() => sendKey(KEYCODE_POWER)}>
          <IconPower size={15} />
          <span>电源</span>
        </button>
        <button className="sidebar-action-btn mini" title="唤醒屏幕" onClick={() => sendKey(KEYCODE_WAKEUP)}>
          <IconSun size={15} />
          <span>唤醒</span>
        </button>
        <button className="sidebar-action-btn mini" title="音量+" onClick={() => sendKey(KEYCODE_VOLUME_UP)}>
          <IconVolumeUp size={15} />
          <span>音量+</span>
        </button>
        <button className="sidebar-action-btn mini" title="音量-" onClick={() => sendKey(KEYCODE_VOLUME_DOWN)}>
          <IconVolumeDown size={15} />
          <span>音量-</span>
        </button>
      </div>

      {/* 屏幕捕获与录制 */}
      <div className="sidebar-key-grid-2">
        <button className="sidebar-action-btn" title="截取屏幕并保存" onClick={doScreenshot}>
          <IconCamera size={15} />
          <span>截屏</span>
        </button>
        <button
          className={`sidebar-action-btn ${recordingScreen ? "recording" : ""}`}
          title={recordingScreen ? "点击停止并保存" : "开始录屏（录当前镜像画面）"}
          onClick={doRecord}
        >
          <IconVideo size={15} />
          <span>{recordingScreen ? `停止保存 (${recElapsed}s)` : "录屏"}</span>
        </button>
      </div>

      {/* 快速文字发送 */}
      <div className="sidebar-text-send-box">
        <input
          className="sidebar-text-input"
          placeholder="向手机发送文字…"
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && sendText()}
        />
        <button className="btn primary icon-only mini" onClick={sendText} title="发送至手机">
          <IconSend size={13} />
        </button>
      </div>

      {!stream.meta && recorder.recording && (
        <div className="badge warn" style={{ alignSelf: "center", marginTop: 4 }}>
          录制中：建议开启投屏以同步触控
        </div>
      )}
    </div>
  );
}
