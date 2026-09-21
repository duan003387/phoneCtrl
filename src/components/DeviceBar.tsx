import type { DeviceInfo } from "../types";

interface Props {
  devices: DeviceInfo[];
  selected: DeviceInfo | null;
  onSelect: (serial: string | null) => void;
  onOpenSettings: () => void;
  onRefresh: () => void;
}

export function DeviceBar({ devices, selected, onSelect, onOpenSettings, onRefresh }: Props) {
  return (
    <div className="device-bar">
      <span className="brand">PhoneCtrl</span>
      <select
        value={selected?.serial ?? ""}
        onChange={(e) => onSelect(e.target.value || null)}
        className="device-select"
      >
        <option value="">选择设备…</option>
        {devices.map((d) => (
          <option key={d.serial} value={d.serial}>
            {d.model || d.product || d.serial} ({d.serial})
          </option>
        ))}
      </select>
      {selected && selected.state !== "device" && (
        <span className="badge warn">离线</span>
      )}
      <button className="btn ghost" onClick={onRefresh} title="刷新设备列表">
        ↻
      </button>
      <button className="btn ghost" onClick={onOpenSettings}>
        设置
      </button>
    </div>
  );
}
