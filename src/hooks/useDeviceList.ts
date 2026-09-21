import { useCallback, useEffect, useRef, useState } from "react";
import { devicesList } from "../api/devices";
import type { DeviceInfo } from "../types";

export interface DeviceSession {
  devices: DeviceInfo[];
  selected: DeviceInfo | null;
  select: (serial: string | null) => void;
  refresh: () => void;
}

/**
 * 轮询设备列表（2s），自动纠正失效的选中设备。
 */
export function useDeviceList(): DeviceSession {
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedSerial, setSelectedSerial] = useState<string | null>(null);
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selectedSerial;

  const refresh = useCallback(async () => {
    try {
      const list = await devicesList();
      setDevices(list);
      const cur = selectedRef.current;
      if (cur && !list.some((d) => d.serial === cur)) {
        setSelectedSerial(null); // 设备已拔出
      }
    } catch {
      setDevices([]);
    }
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, [refresh]);

  const select = useCallback((serial: string | null) => {
    setSelectedSerial(serial);
  }, []);

  const selected = devices.find((d) => d.serial === selectedSerial) ?? null;
  return { devices, selected, select, refresh };
}
