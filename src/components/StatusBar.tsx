import type { DeviceProps } from "../types";
import type { StreamSession } from "../hooks/useStream";

interface Props {
  deviceProps: DeviceProps | null;
  stream: StreamSession;
}

const STREAM_LABEL: Record<string, string> = {
  idle: "未投屏",
  starting: "投屏启动中…",
  streaming: "投屏中",
  error: "投屏异常",
};

export function StatusBar({ deviceProps, stream }: Props) {
  return (
    <div className="status-bar">
      {deviceProps ? (
        <span>
          {deviceProps.manufacturer} {deviceProps.model} · Android {deviceProps.androidVersion}
          {" · "}
          {deviceProps.screenWidth}×{deviceProps.screenHeight}
          {" · "}电量 {deviceProps.batteryLevel}%
          {" · "}
          {deviceProps.awake ? "亮屏" : "熄屏"}
        </span>
      ) : (
        <span>未选择设备</span>
      )}
      <span className={`stream-status ${stream.state}`}>● {STREAM_LABEL[stream.state] ?? stream.state}</span>
    </div>
  );
}
