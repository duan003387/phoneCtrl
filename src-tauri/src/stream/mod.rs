mod control;

use crate::adb::AdbClient;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
pub use control::{ControlClient, ControlSlot, ACTION_DOWN, ACTION_MOVE, ACTION_UP};
use crate::util::now_ms;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::watch;

const SERVER_REMOTE_PATH: &str = "/data/local/tmp/phonectrl-server.jar";
const BRIDGE_REMOTE_PATH: &str = "/data/local/tmp/phonectrl-bridge.jar";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMeta {
    pub serial: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
    pub started_at: u64,
    /// 本地 MJPEG 流地址（前端 <img> 直接播放）
    pub stream_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOpts {
    pub bitrate: Option<u32>,
    pub max_width: Option<u32>,
    pub fps: Option<u32>,
    pub quality: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamStateEvent {
    pub serial: String,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMetaEvent {
    pub serial: String,
    pub width: u32,
    pub height: u32,
}

pub struct StreamHandle {
    /// 流尺寸（scrcpy 服务器实际编码尺寸，旋转/重配置时由流线程更新）
    pub meta: std::sync::RwLock<StreamMeta>,
    pub stop: watch::Sender<bool>,
    /// scrcpy 控制通道（输入注入用）
    pub control: ControlSlot,
    /// 设备物理分辨率（触摸坐标 流→设备 换算用）
    pub screen: (u32, u32),
    /// 系统锁屏（keyguard）是否正在显示：仅用于 scrcpy 通道判定（虚拟显示器
    /// 锁屏忽略注入）。sendevent 桥走内核真实输入不受影响，锁屏亦可注入。
    pub keyguard: std::sync::atomic::AtomicBool,
    /// scrcpy 触摸注入失效标记：部分机型（三星 One UI 8 等）丢弃 scrcpy 服务器的
    /// ASYNC 注入；scrcpy 后端模式下探测确认后不再使用 scrcpy 触摸通道。
    pub touch_adb: std::sync::atomic::AtomicBool,
    /// sendevent 注入桥是否就绪（启动时收到 BRIDGE_READY 即置真，作为触摸首选通道）
    pub bridge_ok: std::sync::atomic::AtomicBool,
    /// 注入桥 stdin（写入事件）与进程句柄
    pub bridge_stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,
    pub bridge_child: std::sync::Mutex<Option<std::process::Child>>,
}

// ────────────────────────── H.264 分发中心（HTTP 服务器数据源） ──────────────────────────

/// 原始 H.264 access unit 广播中心。读流线程推送每个 AU，各 HTTP 客户端订阅。
/// WebCodecs 需要连续、不丢帧的 AU 序列（帧间依赖），故用带背压的多播而非“最新帧槽”。
pub struct H264Hub {
    subs: std::sync::Mutex<Vec<(u64, std::sync::mpsc::SyncSender<Arc<Vec<u8>>>)>>,
    next_id: std::sync::atomic::AtomicU64,
    /// 最近一个关键帧 AU（Annex-B）：新客户端订阅时优先下发，保证 WebCodecs 能立即起步
    last_key: std::sync::Mutex<Option<Arc<Vec<u8>>>>,
    /// 累计 AU 数（触摸注入有效性探测信号）
    pub total: std::sync::atomic::AtomicU64,
}

/// 判断一段 Annex-B access unit 是否含 IDR（NAL 5）——即可作为解码起点的关键帧。
/// 只认 IDR 而非 SPS(7)：scrcpy 会先发一包仅含 SPS/PPS 的配置 AU，那不含图像数据，
/// 不能当作可供 WebCodecs 起步的关键帧缓存。
fn is_keyframe_au(au: &[u8]) -> bool {
    let n = au.len();
    let mut i = 0;
    while i + 2 < n {
        let sc = if au[i] == 0 && au[i + 1] == 0 && au[i + 2] == 1 {
            3
        } else if i + 3 < n && au[i] == 0 && au[i + 1] == 0 && au[i + 2] == 0 && au[i + 3] == 1 {
            4
        } else {
            0
        };
        if sc == 0 {
            i += 1;
            continue;
        }
        let body = i + sc;
        if body < n {
            let t = au[body] & 0x1f;
            if t == 5 {
                return true;
            }
        }
        i = body;
    }
    false
}

impl H264Hub {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            subs: std::sync::Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
            last_key: std::sync::Mutex::new(None),
            total: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// 最近一个关键帧 AU（供新订阅者起步用）。
    fn last_key(&self) -> Option<Arc<Vec<u8>>> {
        self.last_key.lock().unwrap().clone()
    }

    /// 新客户端订阅；返回接收端与其 id（用于注销）。
    fn subscribe(&self) -> (u64, std::sync::mpsc::Receiver<Arc<Vec<u8>>>) {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = std::sync::mpsc::sync_channel::<Arc<Vec<u8>>>(256);
        self.subs.lock().unwrap().push((id, tx));
        (id, rx)
    }

    fn unsubscribe(&self, id: u64) {
        self.subs.lock().unwrap().retain(|(k, _)| *k != id);
    }

    /// 推送一个 AU 给所有订阅者；发送失败（断开/满）的订阅者被清理。
    fn push(&self, au: Arc<Vec<u8>>) {
        self.total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if is_keyframe_au(&au) {
            *self.last_key.lock().unwrap() = Some(au.clone());
        }
        let mut subs = self.subs.lock().unwrap();
        subs.retain(|(_, tx)| match tx.try_send(au.clone()) {
            Ok(()) => true,
            Err(std::sync::mpsc::TrySendError::Full(_)) => true, // 满：丢该客户端这一帧，保留连接
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false,
        });
    }
}

/// 本地 HTTP 服务：以 `Content-Type: video/h264` 流式输出长度前缀封装的
/// Annex-B access unit，供前端 fetch + WebCodecs 解码。
fn spawn_h264_server(listener: TcpListener, hub: Arc<H264Hub>) {
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            let hub = hub.clone();
            std::thread::spawn(move || serve_client(stream, hub));
        }
    });
}

fn serve_client(mut stream: TcpStream, hub: Arc<H264Hub>) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    // 读取并丢弃 HTTP 请求头
    let mut buf = [0u8; 4096];
    let mut req = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                req.extend_from_slice(&buf[..n]);
                if req.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }

    let head = "HTTP/1.1 200 OK\r\nContent-Type: video/h264\r\nCache-Control: no-store\r\n\
                Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }

    // 订阅新客户端。注意：不能排空队列——那会丢掉本会话的起始关键帧。
    let (id, rx) = hub.subscribe();
    // 写一个长度前缀帧的小工具。
    let write_framed = |s: &mut TcpStream, au: &[u8]| -> std::io::Result<()> {
        s.write_all(&(au.len() as u32).to_be_bytes())?;
        s.write_all(au)
    };
    // 若已有缓存 IDR（客户端较晚接入），先下发它让画面立即可见；随后需等到
    // 下一个真正 IDR 再接续实时帧，避免拼接出引用了被跳过帧的损坏画面。
    let mut need_key = false;
    if let Some(key) = hub.last_key() {
        if write_framed(&mut stream, &key).is_err() {
            hub.unsubscribe(id);
            return;
        }
        need_key = true;
    }
    loop {
        match rx.recv() {
            Ok(au) => {
                if need_key && !is_keyframe_au(&au) {
                    continue; // 等下一个 IDR
                }
                need_key = false;
                if write_framed(&mut stream, &au).is_err() {
                    hub.unsubscribe(id);
                    return;
                }
            }
            Err(_) => {
                hub.unsubscribe(id);
                return;
            }
        }
    }
}

// ────────────────────────── 流生命周期 ──────────────────────────

/// 启动投屏流（scrcpy-server：视频 + 控制双通道）。
pub async fn stream_start(
    app: tauri::AppHandle,
    state: &AppState,
    serial: String,
    opts: StreamOpts,
) -> AppResult<StreamMeta> {
    let mut streams = state.streams.lock().await;
    if streams.contains_key(&serial) {
        return Err(AppError::Stream("该设备已有投屏流在运行".into()));
    }

    let cfg = state.config.read().await.clone();
    let adb = state.adb.clone();
    let screen = adb.wm_size(&serial).await?;
    let bitrate = opts.bitrate.unwrap_or(cfg.stream_bitrate);
    let max_width = opts.max_width.unwrap_or(cfg.stream_max_width);
    let fps = opts.fps.unwrap_or(cfg.stream_fps);
    let quality = opts.quality.unwrap_or(cfg.stream_quality);
    let (width, height) = crate::util::scaled_stream_size(screen.0, screen.1, max_width);

    // 绑定本地 HTTP 端口（端口 0 = 系统分配）
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| AppError::Stream(format!("绑定本地流端口失败: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::Stream(e.to_string()))?
        .port();
    let hub = H264Hub::new();
    spawn_h264_server(listener, hub.clone());

    eprintln!(
        "[phonectrl] stream_start serial={serial} screen={screen:?} size={width}x{height} bitrate={bitrate} fps={fps} q={quality} port={port}"
    );

    let meta = StreamMeta {
        serial: serial.clone(),
        width,
        height,
        fps,
        bitrate,
        started_at: now_ms(),
        stream_url: format!("http://127.0.0.1:{port}/h264"),
    };
    let (stop_tx, stop_rx) = watch::channel(false);
    let control = ControlSlot::default();
    let handle = Arc::new(StreamHandle {
        meta: std::sync::RwLock::new(meta.clone()),
        stop: stop_tx,
        control: control.clone(),
        screen,
        keyguard: std::sync::atomic::AtomicBool::new(false),
        touch_adb: std::sync::atomic::AtomicBool::new(false),
        bridge_ok: std::sync::atomic::AtomicBool::new(false),
        bridge_stdin: std::sync::Mutex::new(None),
        bridge_child: std::sync::Mutex::new(None),
    });

    // 锁屏状态轮询：锁屏显示期间，One UI 等会忽略经虚拟显示器转发的触摸事件
    // （scrcpy 把输入注入到镜像虚拟屏），此时输入走 adb 真实显示器回退路径。
    {
        let monitor_adb = adb.clone();
        let monitor_serial = serial.clone();
        let monitor_handle = handle.clone();
        let mut monitor_stop = stop_rx.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(1000));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = monitor_stop.changed() => break,
                    _ = tick.tick() => {}
                }
                if let Ok(out) = monitor_adb
                    .run(Some(&monitor_serial), &["shell", "dumpsys", "window", "policy"])
                    .await
                {
                    let s = &out.stdout;
                    let locked = if let Some(i) = s.find("mIsShowing=") {
                        s[i + "mIsShowing=".len()..].starts_with("true")
                    } else if let Some(i) = s.find("showing=") {
                        s[i + "showing=".len()..].starts_with("true")
                    } else {
                        false
                    };
                    monitor_handle
                        .keyguard
                        .store(locked, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
    }

    tauri::async_runtime::spawn(run_pipeline(
        app.clone(),
        adb,
        serial.clone(),
        handle.clone(),
        stop_rx,
        bitrate,
        fps,
        width,
        height,
        hub,
    ));

    streams.insert(serial, handle);
    Ok(meta)
}

pub async fn stream_stop(state: &AppState, serial: &str) -> AppResult<()> {
    let handle = {
        let mut streams = state.streams.lock().await;
        streams.remove(serial)
    };
    if let Some(h) = handle {
        let _ = h.stop.send(true);
        // 清理设备端 scrcpy 服务器进程（匹配 cmdline 中的 Server 类名）
        let _ = state
            .adb
            .run(Some(serial), &["shell", "pkill", "-9", "-f", "com.genymobile.scrcpy.Server"])
            .await;
    }
    Ok(())
}

pub async fn stream_status(state: &AppState, serial: &str) -> AppResult<Option<StreamMeta>> {
    let streams = state.streams.lock().await;
    Ok(streams.get(serial).map(|h| h.meta.read().unwrap().clone()))
}

/// H.264 源：scrcpy 隧道的视频 TCP 流。
struct Producer {
    adb_child: Child,
    video: TcpStream,
    video_port: u16,
    control_port: u16,
}

async fn run_pipeline(
    app: tauri::AppHandle,
    adb: AdbClient,
    serial: String,
    handle: Arc<StreamHandle>,
    mut stop_rx: watch::Receiver<bool>,
    bitrate: u32,
    fps: u32,
    width: u32,
    height: u32,
    hub: Arc<H264Hub>,
) {
    let mut backoff_ms: u64 = 500;
    emit(&app, &serial, "streaming", None);

    loop {
        if *stop_rx.borrow() {
            break;
        }

        // 每次（含首次）先清理设备上残留的旧 scrcpy 服务器进程。
        // 旧进程会占住 abstract socket 导致新服务器绑定失败（Address already in use），
        // 进而让视频/控制连到僵尸实例。模式必须匹配 cmdline 中的 Server 类名。
        let _ = adb
            .run(Some(&serial), &["shell", "pkill", "-9", "-f", "com.genymobile.scrcpy.Server"])
            .await;
        tokio::time::sleep(Duration::from_millis(150)).await;

        eprintln!("[phonectrl] pipeline iter start");
        let t_spawn = std::time::Instant::now();
        let mut producer = match spawn_scrcpy(
            &adb,
            &serial,
            width,
            height,
            bitrate,
            fps,
            &handle,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[phonectrl] spawn scrcpy 失败: {e}");
                emit(&app, &serial, "error", Some(e.to_string()));
                break;
            }
        };
        eprintln!(
            "[phonectrl] ⏱ spawn_scrcpy(推送+转发+启动+连接) 用时 {:?}",
            t_spawn.elapsed()
        );

        // 读取服务器上报的真实视频尺寸。编码器对齐要求（三星等机型为 16px）
        // 会使实际尺寸与本地推算不同；触摸事件的 screenSize 必须与之一致，
        // 否则服务器 PositionMapper 会丢弃全部触摸事件。
        let real_size = match read_stream_meta(producer.video).await {
            Ok((size, video)) => {
                producer.video = video;
                size
            }
            Err(e) => {
                eprintln!("[phonectrl] 读取视频流元数据失败: {e}");
                emit(&app, &serial, "error", Some(e.to_string()));
                let _ = producer.adb_child.kill();
                let _ = producer.adb_child.wait();
                break;
            }
        };
        if real_size != (width, height) {
            eprintln!(
                "[phonectrl] 服务器实际视频尺寸 {}x{}（本地推算 {width}x{height}）",
                real_size.0, real_size.1
            );
        }
        update_stream_meta(&app, &handle, &serial, real_size);

        eprintln!(
            "[phonectrl] producer 就绪 video_port={} control_port={}",
            producer.video_port, producer.control_port
        );
        let mut producer_adb = producer.adb_child;
        let video_port = producer.video_port;

        // ── 读流线程：解析 scrcpy 帧封装（12 字节头），把每个 Annex-B access unit
        //    原样广播给订阅的 HTTP 客户端，交前端 WebCodecs 硬解。不再经 ffmpeg。──
        let mut video = producer.video;
        let frame_app = app.clone();
        let frame_serial = serial.clone();
        let frame_handle = handle.clone();
        let frame_hub = hub.clone();
        let reader = std::thread::spawn(move || {
            let mut au_count: u64 = 0;
            let mut last_log = std::time::Instant::now();
            loop {
                let mut header = [0u8; 12];
                if video.read_exact(&mut header).is_err() {
                    break;
                }
                let head = u64::from_be_bytes(header[0..8].try_into().unwrap());
                if head & (1u64 << 63) != 0 {
                    // session meta：低 32 位为宽，末 4 字节为高
                    let w = (head & 0xFFFF_FFFF) as u32;
                    let h = u32::from_be_bytes(header[8..12].try_into().unwrap());
                    if w > 0 && h > 0 {
                        eprintln!("[phonectrl] 视频尺寸更新: {w}x{h}");
                        update_stream_meta(&frame_app, &frame_handle, &frame_serial, (w, h));
                    }
                    continue;
                }
                let size = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;
                if size == 0 || size > 32 * 1024 * 1024 {
                    break; // 异常长度：断开重连
                }
                let mut au = vec![0u8; size];
                let mut off = 0;
                let mut eof = false;
                while off < size {
                    match video.read(&mut au[off..]) {
                        Ok(0) | Err(_) => {
                            eof = true;
                            break;
                        }
                        Ok(n) => off += n,
                    }
                }
                if eof {
                    break;
                }
                au_count += 1;
                if au_count == 1 {
                    eprintln!("[phonectrl] 首个 H.264 AU 到达 ({size} 字节)");
                }
                frame_hub.push(Arc::new(au));
                if last_log.elapsed().as_secs() >= 10 {
                    eprintln!("[phonectrl] 10s 内 AU 累计 {au_count}");
                    last_log = std::time::Instant::now();
                }
            }
            eprintln!("[phonectrl] 视频流读取结束, 共 {au_count} AU");
        });

        let stop_changed = stop_rx.changed();
        tokio::pin!(stop_changed);

        let flow = tokio::select! {
            res = tauri::async_runtime::spawn_blocking(move || reader.join()) => {
                match res {
                    Ok(_) => Flow::RestartUnstable,
                    Err(_) => Flow::Stop,
                }
            }
            _ = stop_changed => Flow::Stop,
        };

        // ── 清理设备端进程、隧道与控制通道 ──
        let _ = producer_adb.kill();
        let _ = producer_adb.wait();
        if let Some(mut b) = handle.bridge_child.lock().unwrap().take() {
            let _ = b.kill();
            let _ = b.wait();
            *handle.bridge_stdin.lock().unwrap() = None;
        }
        let _ = adb.run(Some(&serial), &["forward", "--remove", &format!("tcp:{video_port}")]).await;
        let _ = adb
            .run(Some(&serial), &["forward", "--remove", &format!("tcp:{}", control_slot_port(&serial))])
            .await;
        *handle.control.0.lock().await = None;

        match flow {
            Flow::Stop => {
                eprintln!("[phonectrl] 流停止");
                emit(&app, &serial, "stopped", None);
                break;
            }
            Flow::RestartUnstable => {
                eprintln!("[phonectrl] 异常退出, 退避重启 {backoff_ms}ms");
                emit(&app, &serial, "restarting", None);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(2000);
                continue;
            }
        }
    }
}

enum Flow {
    RestartUnstable,
    Stop,
}

/// 每个序列两个端口（视频 + 控制），避免与其它设备冲突。
fn forward_ports(serial: &str) -> (u16, u16) {
    let h = serial.bytes().fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let base = 27184 + (h % 2000) as u16;
    (base, base + 1)
}

fn control_slot_port(serial: &str) -> u16 {
    forward_ports(serial).1
}

// ────────────────────────── 后端：scrcpy-server ──────────────────────────

/// 本地捆绑的 scrcpy-server jar 路径。
fn server_jar_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(dir).join("resources/scrcpy-server-v4.0.jar");
        if p.exists() {
            return Some(p);
        }
    }
    // 打包构建：资源位于可执行文件同级目录
    let p = std::env::current_exe().ok()?.parent()?.join("scrcpy-server-v4.0.jar");
    p.exists().then_some(p)
}

/// 本地捆绑的注入桥 jar 路径。
fn bridge_jar_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(dir).join("resources/inputbridge.jar");
        if p.exists() {
            return Some(p);
        }
    }
    let p = std::env::current_exe().ok()?.parent()?.join("inputbridge.jar");
    p.exists().then_some(p)
}

async fn spawn_scrcpy(
    adb: &AdbClient,
    serial: &str,
    width: u32,
    height: u32,
    bitrate: u32,
    fps: u32,
    handle: &Arc<StreamHandle>,
) -> AppResult<Producer> {
    let jar = server_jar_path()
        .ok_or_else(|| AppError::Stream("未找到 scrcpy-server.jar 资源".into()))?;
    let bridge_jar = bridge_jar_path()
        .ok_or_else(|| AppError::Stream("未找到 inputbridge.jar 资源".into()))?;

    // 推送服务器与注入桥 jar（按需：设备端已存在且大小一致则跳过）。两个推送并发。
    let (push_server, push_bridge) = tokio::join!(
        push_if_changed(adb, serial, &jar, SERVER_REMOTE_PATH, "scrcpy-server"),
        push_if_changed(adb, serial, &bridge_jar, BRIDGE_REMOTE_PATH, "注入桥"),
    );
    push_server?;
    push_bridge?;

    let (video_port, control_port) = forward_ports(serial);
    // 两个端口转发并发建立。
    let fwd_video_arg = format!("tcp:{video_port}");
    let fwd_ctrl_arg = format!("tcp:{control_port}");
    let fwd_v_args = ["forward", fwd_video_arg.as_str(), "localabstract:scrcpy"];
    let fwd_c_args = ["forward", fwd_ctrl_arg.as_str(), "localabstract:scrcpy"];
    let (fwd_v, fwd_c) = tokio::join!(
        adb.run(Some(serial), &fwd_v_args),
        adb.run(Some(serial), &fwd_c_args),
    );
    let fwd_v = fwd_v?;
    let fwd_c = fwd_c?;
    if fwd_v.code != 0 {
        return Err(AppError::Adb(format!("adb forward(视频) 失败: {}", fwd_v.stderr.trim())));
    }
    if fwd_c.code != 0 {
        return Err(AppError::Adb(format!("adb forward(控制) 失败: {}", fwd_c.stderr.trim())));
    }

    // 启动服务器（4.0：raw 视频 + 控制双通道，stay_awake 保持屏幕常亮）。
    // send_stream_meta：上报真实视频尺寸（编码器对齐后的），触摸注入依赖它；
    // send_frame_meta：按帧封装输出，便于拦截 session meta 并把纯 H.264 交给 ffmpeg；
    // capture_orientation=0：锁定竖屏采集。手机平放桌面时传感器可能让系统处于横屏，
    // 且部分机型旋转事件不可靠（scrcpy #5908），锁定后画面方向恒定为竖屏、触摸坐标稳定。
    let cmd = format!(
        "CLASSPATH={SERVER_REMOTE_PATH} app_process / com.genymobile.scrcpy.Server 4.0 \
         log_level=error video=true audio=false control=true tunnel_forward=true \
         send_dummy_byte=true send_device_meta=false send_frame_meta=true send_stream_meta=true \
         stay_awake=true capture_orientation=0 max_size={} video_bit_rate={} max_fps={} \
         video_codec_options=i-frame-interval=1",
        width.max(height),
        bitrate,
        fps
    );
    let child = adb
        .spawn(
            Some(serial),
            &["shell", &cmd],
            Stdio::null(),
            Stdio::null(),
            Stdio::null(),
        )
        .await?;

    // 先连视频通道：必须成功读到 dummy 字节（0x00）才算连接就绪。
    // 服务器 LocalServerSocket 未创建时 adb 转发会立即断开（EOF），需重试。
    let video = connect_video_with_retry(video_port).await?;

    // 连接控制通道并挂到共享槽位（输入注入用）
    match ControlClient::connect(control_port).await {
        Ok(c) => {
            *handle.control.0.lock().await = Some(c);
        }
        Err(e) => {
            eprintln!("[phonectrl] 控制通道连接失败(输入将回退 adb): {e}");
        }
    }

    // 启动常驻 sendevent 注入桥：直写内核 input 设备节点，绕开被 One UI 8 等
    // 机型丢弃的框架层注入。读子进程 stderr，收到 BRIDGE_READY 即确认可用；
    // 收到 BRIDGE_ERR（无触摸设备/无权限）则保持 bridge_ok=false，触摸走后续通道。
    let bridge_cmd =
        format!("CLASSPATH={BRIDGE_REMOTE_PATH} app_process / InputBridge");
    match adb
        .spawn(
            Some(serial),
            &["shell", &bridge_cmd],
            Stdio::piped(),
            Stdio::null(),
            Stdio::piped(),
        )
        .await
    {
        Ok(mut child) => {
            *handle.bridge_stdin.lock().unwrap() = child.stdin.take();
            if let Some(stderr) = child.stderr.take() {
                let bridge_handle = handle.clone();
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    let reader = std::io::BufReader::new(stderr);
                    for line in reader.lines().map_while(Result::ok) {
                        if line.starts_with("BRIDGE_READY") {
                            eprintln!("[phonectrl] {line}");
                            bridge_handle
                                .bridge_ok
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                        } else if line.contains("BRIDGE_ERR") {
                            eprintln!("[phonectrl] 注入桥不可用: {line}");
                        } else {
                            // 其它输出（Java 异常/权限拒绝/崩溃堆栈）也回显，便于诊断
                            eprintln!("[bridge] {line}");
                        }
                    }
                });
            }
            *handle.bridge_child.lock().unwrap() = Some(child);
            eprintln!("[phonectrl] sendevent 注入桥已启动");
        }
        Err(e) => {
            eprintln!("[phonectrl] 注入桥启动失败(触摸将回退 scrcpy/adb): {e}");
        }
    }

    Ok(Producer {
        adb_child: child,
        video,
        video_port,
        control_port,
    })
}

/// 读设备端文件字节数（存在时）。
async fn remote_size(adb: &AdbClient, serial: &str, path: &str) -> Option<u64> {
    let out = adb
        .run(Some(serial), &["shell", "stat", "-c", "%s", path])
        .await
        .ok()?;
    if out.code == 0 {
        out.stdout.trim().parse::<u64>().ok()
    } else {
        None
    }
}

/// 仅当设备端文件缺失或大小与本地不一致时才 push。
/// scrcpy-server 与注入桥 jar 内容固定，重复投屏时这一步能省掉 ~0.3–0.6s 推送往返。
async fn push_if_changed(
    adb: &AdbClient,
    serial: &str,
    local: &std::path::Path,
    remote: &str,
    label: &str,
) -> AppResult<()> {
    let local_len = std::fs::metadata(local).map(|m| m.len()).unwrap_or(0);
    if local_len > 0 && remote_size(adb, serial, remote).await == Some(local_len) {
        return Ok(());
    }
    let out = adb
        .run(Some(serial), &["push", &local.to_string_lossy(), remote])
        .await?;
    if out.code != 0 {
        return Err(AppError::Adb(format!("推送 {label} 失败: {}", out.stderr.trim())));
    }
    Ok(())
}

/// 连接视频通道并读取 dummy 字节；EOF/失败则重试（服务器可能尚未就绪）。
async fn connect_video_with_retry(port: u16) -> AppResult<TcpStream> {
    let mut last = None;
    for _ in 0..25 {
        let res = tauri::async_runtime::spawn_blocking(move || -> AppResult<TcpStream> {
            let mut s = TcpStream::connect(("127.0.0.1", port))?;
            let mut dummy = [0u8; 1];
            s.read_exact(&mut dummy)?;
            Ok(s)
        })
        .await
        .map_err(|e| AppError::Stream(e.to_string()))?;
        match res {
            Ok(s) => return Ok(s),
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    Err(AppError::Stream(format!(
        "连接 scrcpy 视频隧道失败: {}",
        last.map(|e| e.to_string()).unwrap_or_default()
    )))
}

// ────────────────────────── 工具 ──────────────────────────

/// 读取 scrcpy 4.x 视频通道头部：codecId(4) + 首个 session meta(12)。
/// 返回服务器实际编码尺寸（已按编码器对齐要求取整，可能与本地推算不同）。
async fn read_stream_meta(video: TcpStream) -> AppResult<((u32, u32), TcpStream)> {
    let res = tauri::async_runtime::spawn_blocking(move || -> AppResult<((u32, u32), TcpStream)> {
        let mut video = video;
        video
            .set_read_timeout(Some(Duration::from_secs(15)))
            .map_err(|e| AppError::Stream(e.to_string()))?;
        let mut codec = [0u8; 4];
        video
            .read_exact(&mut codec)
            .map_err(|e| AppError::Stream(format!("读取视频流编码头失败: {e}")))?;
        let mut buf = [0u8; 12];
        video
            .read_exact(&mut buf)
            .map_err(|e| AppError::Stream(format!("读取视频流 session meta 失败: {e}")))?;
        video
            .set_read_timeout(None)
            .map_err(|e| AppError::Stream(e.to_string()))?;
        if buf[0] & 0x80 == 0 {
            return Err(AppError::Stream("视频流缺少 session meta".into()));
        }
        let w = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let h = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        if w == 0 || h == 0 {
            return Err(AppError::Stream(format!("视频流尺寸异常: {w}x{h}")));
        }
        Ok(((w, h), video))
    })
    .await
    .map_err(|e| AppError::Stream(e.to_string()))?;
    res
}

/// 更新流尺寸（后端注入与前端坐标映射共用），并通知前端。
fn update_stream_meta(
    app: &tauri::AppHandle,
    handle: &StreamHandle,
    serial: &str,
    (w, h): (u32, u32),
) {
    {
        let mut m = handle.meta.write().unwrap();
        m.width = w;
        m.height = h;
    }
    let _ = app.emit(
        "stream://meta",
        StreamMetaEvent {
            serial: serial.to_string(),
            width: w,
            height: h,
        },
    );
}

fn emit(app: &tauri::AppHandle, serial: &str, state: &str, error: Option<String>) {
    let _ = app.emit(
        "stream://state",
        StreamStateEvent {
            serial: serial.to_string(),
            state: state.to_string(),
            error,
        },
    );
}
