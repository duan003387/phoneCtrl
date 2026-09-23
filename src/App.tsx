import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { MirrorTab } from "./components/MirrorTab";
import { FilePanel } from "./components/FilePanel";
import { AppPanel } from "./components/AppPanel";
import { MacroPanel } from "./components/MacroPanel";
import { AutoPanel } from "./components/AutoPanel";
import { SettingsDialog } from "./components/SettingsDialog";
import { ControlOverlay } from "./components/ControlOverlay";
import { useDeviceList } from "./hooks/useDeviceList";
import { useStream } from "./hooks/useStream";
import { useMacroRecorder } from "./hooks/useMacroRecorder";
import { deviceProps } from "./api/devices";
import type { DeviceProps } from "./types";
import {
  IconMirror,
  IconFolder,
  IconApps,
  IconMacro,
  IconPhone,
  IconRefresh,
  IconSettings,
  IconBattery,
  IconPlay,
} from "./components/Icons";

type Tab = "mirror" | "files" | "apps" | "macros" | "auto";

const TABS: { key: Tab; label: string; icon: React.ComponentType<{ size?: number }> }[] = [
  { key: "mirror", label: "投屏镜像", icon: IconMirror },
  { key: "files", label: "文件管理", icon: IconFolder },
  { key: "apps", label: "应用管理", icon: IconApps },
  { key: "macros", label: "动作宏", icon: IconMacro },
  { key: "auto", label: "自动化", icon: IconPlay },
];

const STREAM_LABEL: Record<string, string> = {
  idle: "未建立流",
  starting: "启动握手中",
  streaming: "低延迟投屏中",
  error: "流异常",
};

function App() {
  const device = useDeviceList();
  const stream = useStream(device.selected?.serial ?? null);
  const recorder = useMacroRecorder();
  const [tab, setTab] = useState<Tab>("mirror");
  const [devicePropsState, setDevicePropsState] = useState<DeviceProps | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [toast, setToast] = useState<{ msg: string; kind: "ok" | "err" } | null>(null);

  const notify = useCallback((msg: string, kind: "ok" | "err" = "ok") => {
    setToast({ msg, kind });
    window.setTimeout(() => setToast(null), 4000);
  }, []);

  const serial = device.selected?.serial ?? null;
  const isOnline = device.selected && device.selected.state === "device";

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
    <div className={`app-workspace ${sidebarOpen ? "with-sidebar" : "sidebar-collapsed"}`}>
      {/* ── 左侧常驻功能与控制侧边栏 ── */}
      {sidebarOpen && (
        <aside className="app-sidebar">
          {/* 顶部品牌与刷新 */}
          <div className="sidebar-brand-row">
            <div className="brand">
              <span className="brand-icon">
                <IconPhone size={16} />
              </span>
              <span>PhoneCtrl</span>
            </div>
            <div className="sidebar-top-actions">
              <button
                className="btn ghost icon-only mini"
                onClick={() => void device.refresh()}
                title="刷新已连接设备"
              >
                <IconRefresh size={14} />
              </button>
              <button
                className="btn ghost icon-only mini"
                onClick={() => setSettingsOpen(true)}
                title="偏好设置"
              >
                <IconSettings size={14} />
              </button>
              <button
                className="btn ghost icon-only mini"
                onClick={() => setSidebarOpen(false)}
                title="折叠侧边栏（纯享全屏手机）"
              >
                <span style={{ fontSize: 13, lineHeight: 1 }}>◀</span>
              </button>
            </div>
          </div>

        {/* 设备选择器卡片 */}
        <div className="sidebar-device-card">
          <div className="device-select-row">
            <span
              className={`device-status-dot ${
                device.selected ? (isOnline ? "online" : "offline") : ""
              }`}
            />
            <select
              value={device.selected?.serial ?? ""}
              onChange={(e) => device.select(e.target.value || null)}
              className="sidebar-device-select"
            >
              <option value="">
                {device.devices.length === 0 ? "未检测到设备…" : "选择目标设备…"}
              </option>
              {device.devices.map((d) => (
                <option key={d.serial} value={d.serial}>
                  {d.model || d.product || d.serial}
                </option>
              ))}
            </select>
          </div>
          {device.selected && (
            <div className="sidebar-device-serial">
              <span>{device.selected.serial}</span>
              <span className={`badge ${isOnline ? "ok" : "warn"}`} style={{ fontSize: 10, padding: "1px 5px" }}>
                {isOnline ? "在线" : "离线"}
              </span>
            </div>
          )}
        </div>

        {/* 垂直导航选项卡 */}
        <nav className="sidebar-nav">
          {TABS.map((t) => {
            const Icon = t.icon;
            const isActive = tab === t.key;
            return (
              <button
                key={t.key}
                className={`sidebar-nav-item ${isActive ? "active" : ""}`}
                onClick={() => setTab(t.key)}
              >
                <Icon size={16} />
                <span>{t.label}</span>
              </button>
            );
          })}
        </nav>

        {/* 手机快捷按键与控制区（当处于投屏 Tab 时呈现） */}
        {tab === "mirror" && serial && (
          <ControlOverlay
            serial={serial}
            stream={stream}
            recorder={recorder}
            notify={notify}
          />
        )}

        {/* 侧边栏底部设备信息与状态指示 */}
        <div className="sidebar-footer">
          {devicePropsState ? (
            <div className="sidebar-device-specs">
              <div className="spec-item">
                <span>{devicePropsState.manufacturer} {devicePropsState.model}</span>
                <span className="spec-battery">
                  <IconBattery size={13} />
                  {devicePropsState.batteryLevel}%
                </span>
              </div>
              <div className="spec-sub">
                <span>Android {devicePropsState.androidVersion}</span>
                <span>{devicePropsState.screenWidth}×{devicePropsState.screenHeight}</span>
              </div>
            </div>
          ) : (
            <div className="sidebar-device-specs">
              <span style={{ color: "var(--text-tertiary)" }}>未选定设备属性</span>
            </div>
          )}

          <div className="sidebar-stream-status">
            <span className={`stream-status-pill ${stream.state}`}>
              ● {STREAM_LABEL[stream.state] ?? stream.state}
            </span>
          </div>
        </div>
      </aside>
      )}

      {/* ── 右侧超大满屏主舞台（从顶到底 100% 满高无遮挡） ── */}
      <main className="app-stage">
        {!sidebarOpen && (
          <button
            className="sidebar-expand-floating-btn"
            onClick={() => setSidebarOpen(true)}
            title="展开控制侧边栏"
          >
            <span>▶ 展开控制侧栏</span>
          </button>
        )}
        {tab === "mirror" && (
          serial ? (
            <MirrorTab
              serial={serial}
              deviceProps={devicePropsState}
              stream={stream}
              recorder={recorder}
              notify={notify}
            />
          ) : (
            <div className="empty-state">
              <IconPhone size={56} className="empty-state-icon" />
              <span>请在左侧侧边栏选择已连接的安卓设备</span>
            </div>
          )
        )}

        {tab === "files" && (
          serial ? (
            <FilePanel serial={serial} notify={notify} />
          ) : (
            <div className="empty-state">
              <IconFolder size={56} className="empty-state-icon" />
              <span>请在左侧侧边栏选择设备以管理文件</span>
            </div>
          )
        )}

        {tab === "apps" && (
          serial ? (
            <AppPanel serial={serial} notify={notify} />
          ) : (
            <div className="empty-state">
              <IconApps size={56} className="empty-state-icon" />
              <span>请在左侧侧边栏选择设备以管理应用</span>
            </div>
          )
        )}

        {tab === "macros" && (
          <MacroPanel serial={serial} notify={notify} />
        )}

        {tab === "auto" && (
          serial ? (
            <AutoPanel serial={serial} notify={notify} />
          ) : (
            <div className="empty-state">
              <IconPlay size={56} className="empty-state-icon" />
              <span>请在左侧侧边栏选择设备以运行自动化用例</span>
            </div>
          )
        )}
      </main>

      {/* 设置弹窗 */}
      {settingsOpen && (
        <SettingsDialog onClose={() => setSettingsOpen(false)} notify={notify} />
      )}

      {/* 消息提示 */}
      {toast && (
        <div className={`toast ${toast.kind}`}>
          <span>{toast.kind === "ok" ? "✓" : "✕"}</span>
          <span>{toast.msg}</span>
        </div>
      )}
    </div>
  );
}

export default App;
