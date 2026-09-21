import type { DeviceProps } from "../types";
import type { StreamSession } from "../hooks/useStream";
import { IconBattery } from "./Icons";

interface Props {
  deviceProps: DeviceProps | null;
  stream: StreamSession;
}

const STREAM_LABEL: Record<string, string> = {
  idle: "未建立视频流",
  starting: "流启动握手中…",
  streaming: "正在以低延迟传输",
  error: "视频流中断或异常",
};

export function StatusBar({ deviceProps, stream }: Props) {
  return (
    <footer className="status-bar">
      <div className="status-left">
        {deviceProps ? (
          <>
            <span className="status-item">
              <strong style={{ color: "var(--text-primary)" }}>
                {deviceProps.manufacturer} {deviceProps.model}
              </strong>
            </span>
            <span className="status-item">
              Android {deviceProps.androidVersion}
            </span>
            <span className="status-item">
              物理 {deviceProps.screenWidth}×{deviceProps.screenHeight}
            </span>
            <span className="status-item" style={{ display: "inline-flex", alignItems: "center", gap: 3 }}>
              <IconBattery size={14} />
              {deviceProps.batteryLevel}%
            </span>
            <span className="status-item">
              <span className={`badge ${deviceProps.awake ? "ok" : "warn"}`}>
                {deviceProps.awake ? "亮屏" : "熄屏休眠"}
              </span>
            </span>
          </>
        ) : (
          <span className="status-item">未连接设备</span>
        )}
      </div>

      <div className="status-right">
        <span className={`stream-status-pill ${stream.state}`}>
          ● {STREAM_LABEL[stream.state] ?? stream.state}
        </span>
      </div>
    </footer>
  );
}
