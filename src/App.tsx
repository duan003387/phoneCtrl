import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { DeviceBar } from "./components/DeviceBar";
import { MirrorTab } from "./components/MirrorTab";
import { FilePanel } from "./components/FilePanel";
import { AppPanel } from "./components/AppPanel";
import { MacroPanel } from "./components/MacroPanel";
import { SettingsDialog } from "./components/SettingsDialog";
import { StatusBar } from "./components/StatusBar";
import { useDeviceList } from "./hooks/useDeviceList";
import { useStream } from "./hooks/useStream";
import { useMacroRecorder } from "./hooks/useMacroRecorder";
import { deviceProps } from "./api/devices";
import type { DeviceProps } from "./types";

type Tab = "mirror" | "files" | "apps" | "macros";

function App() {
  const device = useDeviceList();
  const stream = useStream(device.selected?.serial ?? null);
  const recorder = useMacroRecorder();
  const [tab, setTab] = useState<Tab>("mirror");
  const [devicePropsState, setDevicePropsState] = useState<DeviceProps | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [toast, setToast] = useState<{ msg: string; kind: "ok" | "err" } | null>(null);

  const notify = useCallback((msg: string, kind: "ok" | "err" = "ok") => {
    setToast({ msg, kind });
    window.setTimeout(() => setToast(null), 4000);
  }, []);

  const serial = device.selected?.serial ?? null;

  // 拉取设备详情（随设备与状态变化刷新）
  useEffect(() => {
    if (!serial) {
      setDevicePropsState(null);
      return;
    }
    let alive = true;
    const load = async () => {
      try {
        const p = await deviceProps(serial);
        if (alive) setDevicePropsState(p);
      } catch {
        if (alive) setDevicePropsState(null);
      }
    };
    void load();
    const t = setInterval(load, 5000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [serial, stream.state]);

  return (
    <div className="app">
      <DeviceBar
        devices={device.devices}
        selected={device.selected}
        onSelect={device.select}
        onOpenSettings={() => setSettingsOpen(true)}
        onRefresh={() => void device.refresh()}
      />
      <nav className="tabs">
        {(["mirror", "files", "apps", "macros"] as Tab[]).map((t) => (
          <button
            key={t}
            className={`tab ${tab === t ? "active" : ""}`}
            onClick={() => setTab(t)}
          >
            {{ mirror: "镜像", files: "文件", apps: "应用", macros: "宏" }[t]}
          </button>
        ))}
      </nav>
      <main className="content">
        {tab === "mirror" && (
          serial ? (
            <MirrorTab serial={serial} deviceProps={devicePropsState} stream={stream} recorder={recorder} notify={notify} />
          ) : (
            <div className="empty-state">请在上方选择一台已连接的安卓设备</div>
          )
        )}
        {tab === "files" && (serial ? <FilePanel serial={serial} notify={notify} /> : <div className="empty-state">请先选择设备</div>)}
        {tab === "apps" && (serial ? <AppPanel serial={serial} notify={notify} /> : <div className="empty-state">请先选择设备</div>)}
        {tab === "macros" && <MacroPanel serial={serial} notify={notify} />}
      </main>
      <StatusBar deviceProps={devicePropsState} stream={stream} />
      {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} notify={notify} />}
      {toast && <div className={`toast ${toast.kind}`}>{toast.msg}</div>}
    </div>
  );
}

export default App;
