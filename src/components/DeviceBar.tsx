import type { DeviceInfo } from "../types";
import { IconPhone, IconRefresh, IconSettings } from "./Icons";

interface Props {
  devices: DeviceInfo[];
  selected: DeviceInfo | null;
  onSelect: (serial: string | null) => void;
  onOpenSettings: () => void;
  onRefresh: () => void;
}

export function DeviceBar({ devices, selected, onSelect, onOpenSettings, onRefresh }: Props) {
  const isOnline = selected && selected.state === "device";

  return (
    <header className="device-bar">
      <div className="device-bar-left">
        <div className="brand">
          <span className="brand-icon">
            <IconPhone size={17} />
          </span>
          PhoneCtrl
        </div>

        <div className="device-select-container">
          <span
            className={`device-status-dot ${
              selected ? (isOnline ? "online" : "offline") : ""
            }`}
          />
          <select
            value={selected?.serial ?? ""}
            onChange={(e) => onSelect(e.target.value || null)}
            className="device-select"
          >
            <option value="">
              {devices.length === 0 ? "未检测到安卓设备…" : "选择安卓设备…"}
            </option>
            {devices.map((d) => (
              <option key={d.serial} value={d.serial}>
                {d.model || d.product || d.serial} ({d.serial})
              </option>
            ))}
          </select>
          <span className="select-arrow">▼</span>
        </div>

        {selected && (
          <span className={`badge ${isOnline ? "ok" : "warn"}`}>
            {isOnline ? "已连接" : "离线 / 未授权"}
          </span>
        )}
      </div>

      <div className="device-bar-right">
        <button className="btn ghost" onClick={onRefresh} title="刷新已连接设备列表">
          <IconRefresh size={14} />
          <span>刷新</span>
        </button>
        <button className="btn ghost" onClick={onOpenSettings} title="首选项设置">
          <IconSettings size={14} />
          <span>设置</span>
        </button>
      </div>
    </header>
  );
}
