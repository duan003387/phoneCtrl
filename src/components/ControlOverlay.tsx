import { useState } from "react";
import { key as actionKey, screenshotSave, record } from "../api/actions";
import { text } from "../api/input";
import {
  KEYCODE_HOME,
  KEYCODE_BACK,
  KEYCODE_APP_SWITCH,
  KEYCODE_POWER,
  KEYCODE_WAKEUP,
  KEYCODE_VOLUME_UP,
  KEYCODE_VOLUME_DOWN,
} from "../keycodes";
import type { StreamSession } from "../hooks/useStream";
import type { MacroRecorder } from "../hooks/useMacroRecorder";
import {
  IconBack,
  IconHome,
  IconTasks,
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

export function ControlOverlay({ serial, stream, recorder, notify }: Props) {
  const [inputText, setInputText] = useState("");
  const [recordingScreen, setRecordingScreen] = useState(false);

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

  const doRecord = async () => {
    if (recordingScreen) return;
    setRecordingScreen(true);
    try {
      const path = await record(serial, 15);
      notify(`录屏已保存：${path}`, "ok");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setRecordingScreen(false);
    }
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

      {/* 虚拟导航三键 */}
      <div className="sidebar-key-grid-3">
        <button className="sidebar-action-btn" title="返回键 (Back)" onClick={() => sendKey(KEYCODE_BACK)}>
          <IconBack size={16} />
          <span>返回</span>
        </button>
        <button className="sidebar-action-btn" title="桌面主页 (Home)" onClick={() => sendKey(KEYCODE_HOME)}>
          <IconHome size={16} />
          <span>桌面</span>
        </button>
        <button className="sidebar-action-btn" title="多任务切换 (Recents)" onClick={() => sendKey(KEYCODE_APP_SWITCH)}>
          <IconTasks size={16} />
          <span>多任务</span>
        </button>
      </div>

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
          title="录制屏幕 15 秒"
          onClick={doRecord}
          disabled={recordingScreen}
        >
          <IconVideo size={15} />
          <span>{recordingScreen ? "录制中…" : "录屏 15s"}</span>
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
