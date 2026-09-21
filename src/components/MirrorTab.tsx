import { useCallback } from "react";
import { save as macroSave } from "../api/macros";
import { ControlOverlay } from "./ControlOverlay";
import { MirrorCanvas } from "./MirrorCanvas";
import type { DeviceProps } from "../types";
import type { StreamSession } from "../hooks/useStream";
import type { MacroRecorder } from "../hooks/useMacroRecorder";

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
      notify("未录制到任何动作", "err");
      return;
    }
    const name = window.prompt("宏名称", `宏 ${new Date().toLocaleTimeString()}`);
    if (!name) return;
    try {
      const screenSize: [number, number] | null = deviceProps
        ? [deviceProps.screenWidth, deviceProps.screenHeight]
        : null;
      await macroSave(name, steps, screenSize);
      notify(`宏「${name}」已保存（${steps.length} 个动作）`, "ok");
    } catch (e) {
      notify(String(e), "err");
    }
  }, [recorder, deviceProps, notify]);

  return (
    <div className="mirror-tab">
      <div className="toolbar">
        {stream.state === "idle" || stream.state === "error" ? (
          <button className="btn primary" onClick={() => void stream.start()}>
            开始投屏
          </button>
        ) : (
          <button className="btn" onClick={() => void stream.stop()}>
            停止投屏
          </button>
        )}
        {recorder.recording ? (
          <button className="btn warn" onClick={() => void finishRecording()}>
            停止录制并保存
          </button>
        ) : (
          <button
            className="btn"
            disabled={!stream.meta}
            onClick={recorder.start}
            title="录制后需在「宏」页回放"
          >
            ● 录制宏
          </button>
        )}
        <span className="toolbar-status">
          {stream.meta && (
            <>
              流分辨率 {stream.meta.width}×{stream.meta.height} @ {stream.meta.fps}fps
              {stream.meta.backend === "scrcpy" && "（scrcpy 模式）"}
              {deviceProps && ` · 设备 ${deviceProps.screenWidth}×${deviceProps.screenHeight}`}
            </>
          )}
        </span>
      </div>
      <div className="mirror-stage">
        <MirrorCanvas serial={serial} stream={stream} recorder={recorder} />
        {stream.meta && (
          <ControlOverlay serial={serial} stream={stream} recorder={recorder} notify={notify} />
        )}
      </div>
    </div>
  );
}
