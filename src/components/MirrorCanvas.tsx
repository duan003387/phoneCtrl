import { useEffect, useRef, useState } from "react";
import { touch, key, text } from "../api/input";
import { requestKeyframe } from "../api/stream";
import { KEYMAP } from "../keymap";
import { H264Player } from "../h264player";
import type { StreamSession } from "../hooks/useStream";
import type { MacroRecorder } from "../hooks/useMacroRecorder";
import { IconMirror } from "./Icons";

interface Props {
  serial: string;
  stream: StreamSession;
  recorder: MacroRecorder;
}

const TAP_THRESHOLD = 15;
const MOVE_THROTTLE_MS = 14;

// ── 松手惯性策略 ──
// 惯性本体交给设备端：motionevent 流式注入 / 注入桥 / scrcpy 通道都会把
// 带真实时间戳的事件流交给系统，设备端 VelocityTracker 算出甩动速度后，
// 由应用自身物理产生与真手指一致的惯性滚动（ClampingScrollPhysics 等）。
// 客户端绝不能在松手前合成「指数衰减尾巴」——那会把事件流末段速度衰减到
// 近 0 才抬起，反而掐灭设备端惯性。
// 唯一需要补偿的是鼠标特有的「先停下再松开」：真手指几乎总是在移动中抬起，
// 而鼠标经常停住后再松开按键，设备端速度估计会被停顿清零 → 滚动戛然而止。
// 此时按停顿前的滑动速度补一小段冲刺再抬起，恢复自然惯性。
const FLING_VELOCITY_THRESHOLD = 0.3;   // 流坐标 px/ms，低于此值视为普通拖拽结束
const RESCUE_MAX_PAUSE_MS = 250;        // 停顿超过该值视为主动停住，不补惯性
const RESCUE_STEPS = 5;                 // 补冲刺的 move 条数
const RESCUE_STEP_MS = 14;              // 冲刺事件间隔（贴近设备端注入节奏）

export function MirrorCanvas({ serial, stream, recorder }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [painted, setPainted] = useState(false);

  // WebCodecs 播放器生命周期：随流地址创建/销毁
  useEffect(() => {
    const canvas = canvasRef.current;
    const url = stream.meta?.streamUrl;
    if (!canvas || !url || stream.state !== "streaming") return;
    setPainted(false);
    let player: H264Player | null = null;
    try {
      player = new H264Player(canvas, url);
      player.onFirstFrame = () => setPainted(true);
      player.onRequestKeyframe = () => void requestKeyframe(serial);
      player.start();
    } catch (e) {
      console.error("[h264] 播放器初始化失败", e);
    }
    return () => player?.stop();
  }, [stream.meta?.streamUrl, stream.state, serial]);

  const gesture = useRef<{
    startX: number;
    startY: number;
    x: number;
    y: number;
    moved: boolean;
    lastMoveTs: number;
    downTs: number;
    samples: { t: number; x: number; y: number }[];
  } | null>(null);

  /** 窗口内首末样本平均速度（流坐标 px/ms 向量）。样本不足时返回零向量。 */
  const windowVelocity = (
    samples: { t: number; x: number; y: number }[],
    untilTs: number,
    windowMs = 120,
  ) => {
    const recent = samples.filter((s) => s.t <= untilTs && untilTs - s.t <= windowMs);
    if (recent.length < 2) return { vx: 0, vy: 0 };
    const a = recent[0];
    const b = recent[recent.length - 1];
    const dt = b.t - a.t;
    if (dt < 20) return { vx: 0, vy: 0 };
    return { vx: (b.x - a.x) / dt, vy: (b.y - a.y) / dt };
  };

  /** 短暂停顿后松手的补偿冲刺：按停顿前速度继续滑动一小段再抬起。
      不做代次中止——全程仅 ~70ms 且必以 up 收尾，新手势的 down 会在
      通道中排队跟上，保证设备端触摸状态始终自洽（down↔up 成对）。 */
  const sendRescueBurst = async (
    serial_: string,
    x: number,
    y: number,
    vx: number,
    vy: number,
  ) => {
    let px = x;
    let py = y;
    for (let i = 0; i < RESCUE_STEPS; i++) {
      await new Promise((r) => setTimeout(r, RESCUE_STEP_MS));
      px += vx * RESCUE_STEP_MS;
      py += vy * RESCUE_STEP_MS;
      void touch(serial_, "move", Math.round(px), Math.round(py));
    }
    void touch(serial_, "up", Math.round(px), Math.round(py));
  };

  /** 客户端坐标 → 视频流坐标（0..流宽, 0..流高）。
      必须用流坐标系：scrcpy 服务器要求事件尺寸与视频尺寸一致，否则丢弃事件。 */
  const toDevice = (clientX: number, clientY: number): [number, number] | null => {
    const el = containerRef.current;
    if (!el || !stream.meta) return null;
    const rect = el.getBoundingClientRect();
    const cw = rect.width;
    const ch = rect.height;
    const fw = stream.meta.width;
    const fh = stream.meta.height;
    if (fw === 0 || fh === 0) return null;

    const scale = Math.min(cw / fw, ch / fh);
    const dw = fw * scale;
    const dh = fh * scale;
    const dx = (cw - dw) / 2;
    const dy = (ch - dh) / 2;

    const px = clientX - rect.left;
    const py = clientY - rect.top;
    const fx = (px - dx) / dw;
    const fy = (py - dy) / dh;
    if (fx < 0 || fy < 0 || fx > 1 || fy > 1) return null;
    return [Math.round(fx * fw), Math.round(fy * fh)];
  };

  const onPointerDown = (e: React.PointerEvent) => {
    e.preventDefault();
    if (!stream.meta) return;
    const pt = toDevice(e.clientX, e.clientY);
    if (!pt) return;
    (e.target as Element).setPointerCapture?.(e.pointerId);
    gesture.current = {
      startX: pt[0], startY: pt[1],
      x: pt[0], y: pt[1],
      moved: false, lastMoveTs: 0, downTs: performance.now(), samples: [],
    };
    void touch(serial, "down", pt[0], pt[1]);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const g = gesture.current;
    if (!g || !stream.meta) return;
    const pt = toDevice(e.clientX, e.clientY);
    if (!pt) return;
    const dx = pt[0] - g.startX;
    const dy = pt[1] - g.startY;
    if (dx * dx + dy * dy > TAP_THRESHOLD * TAP_THRESHOLD) g.moved = true;
    g.x = pt[0];
    g.y = pt[1];
    const now = performance.now();
    g.samples.push({ t: now, x: pt[0], y: pt[1] });
    while (g.samples.length > 24) g.samples.shift();
    if (g.moved && now - g.lastMoveTs >= MOVE_THROTTLE_MS) {
      g.lastMoveTs = now;
      void touch(serial, "move", pt[0], pt[1]);
    }
  };

  const onPointerUp = (e: React.PointerEvent) => {
    e.preventDefault();
    releasePointer(e);
  };

  /** 指针释放/取消：必须补发 up，否则设备侧手指保持按下，后续所有触摸失效。
      指针移出画面时 toDevice 返回 null，此时退回最后一次有效坐标。
      惯性由设备端根据事件流速度自行产生（见文件头「松手惯性策略」），
      这里只处理鼠标「停下再松开」的补偿冲刺。 */
  const releasePointer = (e: React.PointerEvent) => {
    e.preventDefault();
    const g = gesture.current;
    gesture.current = null;
    if (!g || !stream.meta) return;
    const pt = toDevice(e.clientX, e.clientY) ?? [g.x, g.y];
    const gestureMs = Math.max(80, Math.min(400, performance.now() - g.downTs));

    // 宏录制：聚合成 tap/swipe 记录
    if (g.moved) {
      recorder.push({
        type: "swipe",
        x1: g.startX,
        y1: g.startY,
        x2: pt[0],
        y2: pt[1],
        durationMs: gestureMs,
      });
    } else {
      recorder.push({ type: "tap", x: pt[0], y: pt[1] });
    }

    if (!g.moved) {
      void touch(serial, "up", pt[0], pt[1]);
      return;
    }

    const last = g.samples.length ? g.samples[g.samples.length - 1] : null;
    const sinceMove = last ? performance.now() - last.t : Infinity;

    // 松手时鼠标仍在滑动（≤100ms 内有移动）：设备端已看到完整速度轨迹，
    // 直接抬起即可获得原生惯性。
    if (last && sinceMove <= 100) {
      void touch(serial, "up", pt[0], pt[1]);
      return;
    }

    // 短暂停顿（≤250ms）后松开：按停顿前的速度补冲刺，恢复设备端速度估计
    if (last && sinceMove <= RESCUE_MAX_PAUSE_MS) {
      const v = windowVelocity(g.samples, last.t);
      const speed = Math.hypot(v.vx, v.vy);
      if (speed >= FLING_VELOCITY_THRESHOLD) {
        void sendRescueBurst(serial, pt[0], pt[1], v.vx, v.vy);
        return;
      }
    }

    // 慢速拖拽 / 长停顿：与真手指停住后抬起一致，直接抬起（无惯性）
    void touch(serial, "up", pt[0], pt[1]);
  };

  const onWheel = (e: React.WheelEvent) => {
    if (!stream.meta) return;
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const c = toDevice(cx, cy);
    if (!c) return;
    const step = 300;
    void touch(serial, "down", c[0], c[1]);
    if (e.deltaY > 0) {
      void touch(serial, "move", c[0], c[1] - step);
    } else {
      void touch(serial, "move", c[0], c[1] + step);
    }
    void touch(serial, "up", c[0], e.deltaY > 0 ? c[1] - step : c[1] + step);
  };

  // 键盘控制
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (!stream.meta || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === "Tab" || e.key === "ArrowUp" || e.key === "ArrowDown") e.preventDefault();
      if (e.key.length === 1 && e.key !== " ") {
        void text(serial, e.key);
        recorder.push({ type: "text", text: e.key });
        return;
      }
      const code = KEYMAP[e.key];
      if (code !== undefined) {
        void key(serial, code);
        recorder.push({ type: "key", keycode: code });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [serial, stream.meta, recorder]);

  const showPlaceholder =
    stream.state !== "streaming" || !stream.meta || (stream.state === "streaming" && !painted);

  return (
    <div className="mirror-wrap">
      {recorder.recording && (
        <div className="rec-badge">
          ● 宏录制中 {Math.round(recorder.elapsed / 1000)}s
        </div>
      )}
      <div
        ref={containerRef}
        className={`mirror-view ${stream.state === "streaming" && stream.meta ? "active" : ""}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={releasePointer}
        onLostPointerCapture={releasePointer}
        onWheel={onWheel}
      >
        {stream.meta && (
          <canvas
            ref={canvasRef}
            className="mirror-img"
            style={{ opacity: painted ? 1 : 0 }}
          />
        )}
        {showPlaceholder && (
          <div className="mirror-placeholder">
            <IconMirror size={42} style={{ opacity: 0.4 }} />
            {stream.state === "starting" && <span>正在启动 H.264 视频流…</span>}
            {stream.state === "streaming" && !painted && <span>正在解码建立首帧画面…</span>}
            {stream.state === "idle" && <span>点击左上方「开始投屏」连接设备屏幕</span>}
            {stream.state === "error" && (
              <span className="err-text">投屏启动异常：{stream.error}</span>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
