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
    <div className="control-overlay">
      <div className="ctl-group">
        <button className="ctl-btn" title="电源" onClick={() => sendKey(KEYCODE_POWER)}>⏻</button>
        <button className="ctl-btn" title="唤醒" onClick={() => sendKey(KEYCODE_WAKEUP)}>☀</button>
        <button className="ctl-btn" title="音量+" onClick={() => sendKey(KEYCODE_VOLUME_UP)}>🔊+</button>
        <button className="ctl-btn" title="音量-" onClick={() => sendKey(KEYCODE_VOLUME_DOWN)}>🔊−</button>
      </div>
      <div className="ctl-group">
        <button className="ctl-btn" title="返回" onClick={() => sendKey(KEYCODE_BACK)}>◀</button>
        <button className="ctl-btn" title="桌面" onClick={() => sendKey(KEYCODE_HOME)}>⌂</button>
        <button className="ctl-btn" title="最近任务" onClick={() => sendKey(KEYCODE_APP_SWITCH)}>▦</button>
      </div>
      <div className="ctl-group">
        <button className="ctl-btn" title="截屏并保存" onClick={doScreenshot}>📷</button>
        <button className="ctl-btn" title="录屏 15 秒" onClick={doRecord} disabled={recordingScreen}>
          {recordingScreen ? "…" : "⏺"}
        </button>
      </div>
      <div className="ctl-group">
        <input
          className="ctl-text"
          placeholder="输入文本（支持中文）"
          value={inputText}
          onChange={(e) => setInputText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && sendText()}
        />
        <button className="ctl-btn" onClick={sendText}>发送</button>
      </div>
      {!stream.meta && recorder.recording && (
        <div className="ctl-hint">录制中：请先在设备上开始投屏</div>
      )}
    </div>
  );
}
