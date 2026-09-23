import { useCallback } from "react";
import { save as macroSave } from "../api/macros";
import { key as actionKey } from "../api/actions";
import {
  KEYCODE_BACK,
  KEYCODE_HOME,
  KEYCODE_APP_SWITCH,
} from "../keycodes";
import { MirrorCanvas } from "./MirrorCanvas";
import { promptText } from "../ui/dialogs";
import type { DeviceProps } from "../types";
import type { StreamSession } from "../hooks/useStream";
import type { MacroRecorder } from "../hooks/useMacroRecorder";
import {
  IconPlay,
  IconStop,
  IconMacro,
  IconBack,
  IconHome,
  IconTasks,
} from "./Icons";

interface Props {
  serial: string;
  deviceProps: DeviceProps | null;
  stream: StreamSession;
  recorder: MacroRecorder;
  notify: (msg: string, kind?: "ok" | "err") => void;
}

export function MirrorTab({ serial, deviceProps, stream, recorder, notify }: Props) {
  const finishRecording = useCallback(async () => {
    const steps = recorder.finish();
    if (steps.length === 0) {
      notify("未录制到任何动作（请在画面上点击/滑动后再保存）", "err");
      return;
    }
    const name = await promptText("宏名称", `宏 ${new Date().toLocaleTimeString()}`, "宏名称");
    if (!name) return;
    try {
      const screenSize: [number, number] | null = deviceProps
        ? [deviceProps.screenWidth, deviceProps.screenHeight]
        : null;
      await macroSave(name, steps, screenSize);
      notify(`宏「${name}」已保存（${steps.length} 个动作），见「宏」页`, "ok");
    } catch (e) {
      notify(String(e), "err");
    }
  }, [recorder, deviceProps, notify]);

  const isStreaming = stream.state === "streaming";

  const sendKey = (keycode: number) => {
    void actionKey(serial, keycode);
    recorder.push({ type: "key", keycode });
  };

  return (
    <div className="mirror-stage-container">
      {/* 顶部独立工具栏（标准流占位，绝对不覆盖手机画面） */}
      <div className="mirror-header-bar">
        <div className="mirror-header-left">
          {stream.state === "idle" || stream.state === "error" ? (
            <button className="btn primary mini" onClick={() => void stream.start()}>
              <IconPlay size={13} />
              <span>开始投屏</span>
            </button>
          ) : (
            <button className="btn mini" onClick={() => void stream.stop()}>
              <IconStop size={13} />
              <span>停止投屏</span>
            </button>
          )}

          {recorder.recording ? (
            <button className="btn warn mini" onClick={() => void finishRecording()}>
              <IconStop size={13} />
              <span>保存宏</span>
            </button>
          ) : (
            <button
              className="btn mini"
              disabled={!isStreaming}
              onClick={recorder.start}
              title="录制动作序列"
            >
              <IconMacro size={13} />
              <span>录制宏</span>
            </button>
          )}
        </div>

        <div className="mirror-header-right">
          {stream.meta ? (
            <div className="stream-info-pills">
              <span className="pill-tag">
                {stream.meta.width}×{stream.meta.height}
              </span>
              <span className="pill-tag">{stream.meta.fps} FPS</span>
              <span className="pill-tag">
                {stream.meta.backend === "scrcpy" ? "scrcpy 协议" : "adb 模式"}
              </span>
            </div>
          ) : (
            <span className="pill-tag">等待投屏</span>
          )}
        </div>
      </div>

      {/* 手机画面区域：自适应剩余全部高度，与顶部操作栏完全隔离无遮挡 */}
      <div className="mirror-canvas-container">
        <MirrorCanvas serial={serial} stream={stream} recorder={recorder} />
      </div>

      {/* 底部虚拟导航三键：独立成行（不覆盖画面），仅图标 */}
      <div className="mirror-nav-bar">
        <button className="nav-btn" title="返回" onClick={() => sendKey(KEYCODE_BACK)}>
          <IconBack size={20} />
        </button>
        <button className="nav-btn" title="桌面" onClick={() => sendKey(KEYCODE_HOME)}>
          <IconHome size={20} />
        </button>
        <button className="nav-btn" title="多任务" onClick={() => sendKey(KEYCODE_APP_SWITCH)}>
          <IconTasks size={20} />
        </button>
      </div>
    </div>
  );
}
