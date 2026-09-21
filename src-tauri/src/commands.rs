use crate::config::{self, AppConfig};
use crate::error::{AppError, AppResult};
use crate::macros::{Macro, MacroStep, MacroStepAt};
use crate::state::AppState;
use crate::stream::{self, StreamMeta, StreamOpts};
use std::io::Write;
use tauri::ipc::Channel;
use tauri::State;
use tokio::time::Duration;

// ────────────────────────── 设备管理 ──────────────────────────

#[tauri::command]
pub async fn devices_list(state: State<'_, AppState>) -> AppResult<Vec<crate::adb::devices::DeviceInfo>> {
    state.adb.list_devices().await
}

#[tauri::command]
pub async fn device_start_server(state: State<'_, AppState>) -> AppResult<()> {
    state.adb.ensure_server().await
}

#[tauri::command]
pub async fn device_props(state: State<'_, AppState>, serial: String) -> AppResult<crate::adb::devices::DeviceProps> {
    state.adb.props(&serial).await
}

// ────────────────────────── 设置 ──────────────────────────

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> AppResult<AppConfig> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, config: AppConfig) -> AppResult<()> {
    // 校验并更新 adb 路径；ffmpeg 已从投屏管线移除，仅在提供时更新其路径，不强制存在。
    let adb_path = config::detect_adb(config.adb_path.as_deref())?;
    if let Some(p) = config.ffmpeg_path.as_deref() {
        if let Ok(ffmpeg) = config::detect_ffmpeg(Some(p)) {
            *state.ffmpeg_path.write().await = ffmpeg;
        }
    }
    state.adb.set_path(adb_path).await;
    config::save(&state.config_dir, &config)?;
    *state.config.write().await = config;
    Ok(())
}

#[tauri::command]
pub async fn diagnostics(state: State<'_, AppState>) -> AppResult<String> {
    let cfg = state.config.read().await.clone();
    let adb_path = config::detect_adb(cfg.adb_path.as_deref())?;
    let ver = state.adb.run(None, &["version"]).await?;
    let dev_count = state.adb.list_devices().await?.len();
    Ok(format!(
        "adb: {}\n{}\n已连接设备数: {dev_count}",
        adb_path.display(),
        ver.stdout.trim(),
    ))
}

// ────────────────────────── 投屏流 ──────────────────────────

#[tauri::command]
pub async fn stream_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    serial: String,
    opts: StreamOpts,
) -> AppResult<StreamMeta> {
    stream::stream_start(app, &state, serial, opts).await
}

#[tauri::command]
pub async fn stream_stop(state: State<'_, AppState>, serial: String) -> AppResult<()> {
    stream::stream_stop(&state, &serial).await
}

#[tauri::command]
pub async fn stream_status(state: State<'_, AppState>, serial: String) -> AppResult<Option<StreamMeta>> {
    stream::stream_status(&state, &serial).await
}

// ────────────────────────── 输入控制 ──────────────────────────

use crate::stream::{ACTION_DOWN, ACTION_MOVE, ACTION_UP};

/// 流坐标 → 设备像素坐标（触摸桥 / adb 注入使用设备坐标系）。
fn scale_to_device(h: &crate::stream::StreamHandle, x: u32, y: u32) -> (u32, u32) {
    let (dw, dh) = {
        let m = h.meta.read().unwrap();
        (m.width.max(1), m.height.max(1))
    };
    let (sw, sh) = h.screen;
    ((x * sw + dw / 2) / dw, (y * sh + dh / 2) / dh)
}

/// 通过注入桥发送触摸事件（设备像素坐标）。
/// sendevent 桥协议：[type u8][x i32 BE][y i32 BE]，type: 1=down 2=move 3=up。
fn bridge_touch(
    h: &crate::stream::StreamHandle,
    action: u8,
    x: u32,
    y: u32,
) -> std::io::Result<()> {
    let ty = match action {
        ACTION_DOWN => 1u8,
        ACTION_UP => 3u8,
        ACTION_MOVE => 2u8,
        _ => return Ok(()),
    };
    let mut buf = Vec::with_capacity(9);
    buf.push(ty);
    buf.extend_from_slice(&(x as i32).to_be_bytes());
    buf.extend_from_slice(&(y as i32).to_be_bytes());
    let mut guard = h.bridge_stdin.lock().unwrap();
    if let Some(stdin) = guard.as_mut() {
        stdin.write_all(&buf)?;
        stdin.flush()?;
    }
    Ok(())
}

/// 通过 scrcpy 控制通道发送触摸事件序列；返回是否已处理（否则回退 adb）。
/// 注意：坐标必须使用视频流坐标系（0..流宽, 0..流高），screenSize 必须等于
/// 服务器当前视频尺寸（取自服务器 session meta，含编码器对齐），
/// 否则 PositionMapper 会丢弃事件。
/// 锁屏显示或触摸注入已确认失效的设备返回 false，由调用方走 adb 注入。
async fn touch_via_socket(
    state: &AppState,
    serial: &str,
    events: &[(u8, u32, u32)],
) -> AppResult<bool> {
    let streams = state.streams.lock().await;
    if let Some(h) = streams.get(serial) {
        // 锁屏显示期间，One UI 等锁屏会忽略经虚拟显示器转发的触摸事件，
        // 回退到 adb 注入（作用于真实显示器，锁屏可响应）
        if h.keyguard.load(std::sync::atomic::Ordering::Relaxed)
            || h.touch_adb.load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(false);
        }
        let mut guard = h.control.0.lock().await;
        if let Some(c) = guard.as_mut() {
            let (sw, sh) = {
                let m = h.meta.read().unwrap();
                (m.width as u16, m.height as u16)
            };
            for (i, (a, x, y)) in events.iter().enumerate() {
                if i > 0 {
                    tokio::time::sleep(Duration::from_millis(8)).await;
                }
                c.touch(*a, *x, *y, sw, sh).await?;
            }
            return Ok(true);
        }
    }
    Ok(false)
}

async fn key_via_socket(state: &AppState, serial: &str, keycode: u16) -> AppResult<bool> {
    let streams = state.streams.lock().await;
    if let Some(h) = streams.get(serial) {
        // scrcpy 通道可用（未确认失效）且未锁屏：走 scrcpy 按键；
        // 否则返回 false 由调用方回退 adb keyevent。
        if !h.keyguard.load(std::sync::atomic::Ordering::Relaxed)
            && !h.touch_adb.load(std::sync::atomic::Ordering::Relaxed)
        {
            let mut guard = h.control.0.lock().await;
            if let Some(c) = guard.as_mut() {
                c.keycode(keycode as u32).await?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

async fn text_via_socket(state: &AppState, serial: &str, text: &str) -> AppResult<bool> {
    let streams = state.streams.lock().await;
    if let Some(h) = streams.get(serial) {
        if h.keyguard.load(std::sync::atomic::Ordering::Relaxed)
            || h.touch_adb.load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(false);
        }
        let mut guard = h.control.0.lock().await;
        if let Some(c) = guard.as_mut() {
            if text.is_ascii() {
                c.text(text).await?;
            } else {
                c.clipboard_and_paste(text).await?;
            }
            return Ok(true);
        }
    }
    Ok(false)
}

#[tauri::command]
pub async fn input_tap(state: State<'_, AppState>, serial: String, x: u32, y: u32) -> AppResult<()> {
    if touch_via_socket(&state, &serial, &[(ACTION_DOWN, x, y), (ACTION_UP, x, y)]).await? {
        return Ok(());
    }
    let (dx, dy) = device_coords(&state, &serial, x, y).await;
    state.input.tap(&state.adb, &serial, dx, dy).await
}

#[tauri::command]
pub async fn input_swipe(
    state: State<'_, AppState>,
    serial: String,
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    duration_ms: u32,
) -> AppResult<()> {
    if touch_via_socket(
        &state,
        &serial,
        &[
            (ACTION_DOWN, x1, y1),
            (ACTION_MOVE, (x1 + x2) / 2, (y1 + y2) / 2),
            (ACTION_UP, x2, y2),
        ],
    )
    .await?
    {
        // socket 注入瞬时完成，等待接近前端期望的时长，保证滑动手感
        if duration_ms > 30 {
            tokio::time::sleep(Duration::from_millis((duration_ms / 3).min(150) as u64)).await;
        }
        return Ok(());
    }
    let (dx1, dy1) = device_coords(&state, &serial, x1, y1).await;
    let (dx2, dy2) = device_coords(&state, &serial, x2, y2).await;
    state
        .input
        .swipe(&state.adb, &serial, dx1, dy1, dx2, dy2, duration_ms)
        .await
}

/// 流坐标 → 设备坐标（无流时原样返回）。
async fn device_coords(state: &AppState, serial: &str, x: u32, y: u32) -> (u32, u32) {
    let streams = state.streams.lock().await;
    match streams.get(serial) {
        Some(h) => scale_to_device(h, x, y),
        None => (x, y),
    }
}

#[tauri::command]
pub async fn input_key(state: State<'_, AppState>, serial: String, keycode: u16) -> AppResult<()> {
    if key_via_socket(&state, &serial, keycode).await? {
        return Ok(());
    }
    state.input.keyevent(&state.adb, &serial, keycode).await
}

#[tauri::command]
pub async fn input_text(state: State<'_, AppState>, serial: String, text: String) -> AppResult<()> {
    if text_via_socket(&state, &serial, &text).await? {
        return Ok(());
    }
    if text.is_ascii() {
        state.input.text(&state.adb, &serial, &text).await
    } else {
        state.input.text_unicode(&state.adb, &serial, &text).await
    }
}

/// 实时触摸事件（down/move/up），前端拖拽用。
///
/// 注入通道优先级（sendevent 桥在启动时已通过 BRIDGE_READY 确认，无需运行时探测）：
///   1. sendevent 注入桥 —— 直写内核 input 设备，实时、可靠，含锁屏；绕开被
///      三星 One UI 8 等机型丢弃的框架层注入。
///   2. scrcpy 控制通道 —— 桥不可用时使用（延迟也不高）；首个手势探测其有效性，
///      若该机丢弃 scrcpy 注入则标记失效。
///   3. 逐手势 adb —— 前两者都不可用时的最终回退（`input swipe/tap`，最慢但最通用）。
#[tauri::command]
pub async fn input_touch(
    state: State<'_, AppState>,
    serial: String,
    action: String,
    x: u32,
    y: u32,
) -> AppResult<()> {
    let action_id = match action.as_str() {
        "down" => ACTION_DOWN,
        "up" => ACTION_UP,
        "move" => ACTION_MOVE,
        _ => return Err(AppError::Device(format!("未知触摸动作: {action}"))),
    };

    let streams = state.streams.lock().await;
    let Some(h) = streams.get(&serial) else {
        return Ok(());
    };
    let keyguard = h.keyguard.load(std::sync::atomic::Ordering::Relaxed);
    let scrcpy_dead = h.touch_adb.load(std::sync::atomic::Ordering::Relaxed);
    let bridge_ok = h.bridge_ok.load(std::sync::atomic::Ordering::Relaxed);

    // ── 通道 1：sendevent 注入桥（首选，实时转发每个事件，锁屏亦可） ──
    if bridge_ok {
        let (dx, dy) = scale_to_device(h, x, y);
        if let Err(e) = bridge_touch(h, action_id, dx, dy) {
            eprintln!("[phonectrl] 注入桥写入失败，回退其它通道: {e}");
            h.bridge_ok
                .store(false, std::sync::atomic::Ordering::Relaxed);
        } else {
            return Ok(());
        }
    }

    // ── 通道 2：scrcpy 控制通道（桥不可用时；带首手势探测） ──
    if !keyguard && !scrcpy_dead {
        let mut guard = h.control.0.lock().await;
        if let Some(c) = guard.as_mut() {
            let (sw, sh) = {
                let m = h.meta.read().unwrap();
                (m.width as u16, m.height as u16)
            };
            c.touch(action_id, x, y, sw, sh).await?;

            // 探测（首个手势结束）：画面零变化 = 该机型丢弃 scrcpy 注入
            if !h.touch_probe_done.load(std::sync::atomic::Ordering::Relaxed) {
                match action_id {
                    ACTION_DOWN => {
                        *h.down_info.lock().unwrap() = Some((
                            x,
                            y,
                            h.frames.total.load(std::sync::atomic::Ordering::Relaxed),
                        ));
                    }
                    ACTION_UP => {
                        h.touch_probe_done
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        let down = h.down_info.lock().unwrap().take();
                        if let Some((ox, oy, fr0)) = down {
                            let deadline =
                                std::time::Instant::now() + Duration::from_millis(240);
                            let mut delta = h
                                .frames
                                .total
                                .load(std::sync::atomic::Ordering::Relaxed)
                                .saturating_sub(fr0);
                            while delta < 2 && std::time::Instant::now() < deadline {
                                tokio::time::sleep(Duration::from_millis(80)).await;
                                delta = h
                                    .frames
                                    .total
                                    .load(std::sync::atomic::Ordering::Relaxed)
                                    .saturating_sub(fr0);
                            }
                            if delta < 2 {
                                h.touch_adb
                                    .store(true, std::sync::atomic::Ordering::Relaxed);
                                eprintln!(
                                    "[phonectrl] scrcpy 触摸注入无效（手势后画面无帧变化），回退逐手势 adb"
                                );
                                drop(guard);
                                drop(streams);
                                let moved = x.abs_diff(ox) + y.abs_diff(oy) > 30;
                                if moved {
                                    let (sx0, sy0) = device_coords(&state, &serial, ox, oy).await;
                                    let (sx, sy) = device_coords(&state, &serial, x, y).await;
                                    state
                                        .input
                                        .swipe(&state.adb, &serial, sx0, sy0, sx, sy, 250)
                                        .await?;
                                } else {
                                    let (sx, sy) = device_coords(&state, &serial, x, y).await;
                                    state.input.tap(&state.adb, &serial, sx, sy).await?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
                return Ok(());
            }
            return Ok(());
        }
    }

    // ── 通道 3：逐手势 adb（down 记录起点，up 聚合 tap/swipe） ──
    drop(streams);
    eprintln!("[phonectrl] input_touch {action} ({x},{y}) via adb");
    let mut origins = state.touch_origins.lock().await;
    match action_id {
        ACTION_DOWN => {
            origins.insert(serial.clone(), (x, y));
        }
        ACTION_UP => {
            if let Some((ox, oy)) = origins.remove(&serial) {
                let moved = x.abs_diff(ox) + y.abs_diff(oy) > 30;
                let streams = state.streams.lock().await;
                if let Some(h) = streams.get(&serial) {
                    let (sx0, sy0) = scale_to_device(h, ox, oy);
                    let (sx, sy) = scale_to_device(h, x, y);
                    drop(streams);
                    if moved {
                        state.input.swipe(&state.adb, &serial, sx0, sy0, sx, sy, 200).await?;
                    } else {
                        state.input.tap(&state.adb, &serial, sx, sy).await?;
                    }
                } else {
                    drop(streams);
                    if moved {
                        state.input.swipe(&state.adb, &serial, ox, oy, x, y, 200).await?;
                    } else {
                        state.input.tap(&state.adb, &serial, x, y).await?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ────────────────────────── 文件管理 ──────────────────────────

#[tauri::command]
pub async fn files_list(
    state: State<'_, AppState>,
    serial: String,
    path: String,
) -> AppResult<Vec<crate::adb::files::FileEntry>> {
    state.adb.list_files(&serial, &path).await
}

#[tauri::command]
pub async fn files_delete(state: State<'_, AppState>, serial: String, path: String) -> AppResult<()> {
    state.adb.delete_path(&serial, &path).await
}

#[tauri::command]
pub async fn files_mkdir(state: State<'_, AppState>, serial: String, path: String) -> AppResult<()> {
    state.adb.mkdir_path(&serial, &path).await
}

#[tauri::command]
pub async fn files_rename(
    state: State<'_, AppState>,
    serial: String,
    from: String,
    to: String,
) -> AppResult<()> {
    state.adb.rename_path(&serial, &from, &to).await
}

#[tauri::command]
pub async fn files_copy(
    state: State<'_, AppState>,
    serial: String,
    from: String,
    to: String,
) -> AppResult<()> {
    state.adb.copy_path(&serial, &from, &to).await
}

#[tauri::command]
pub async fn files_upload(
    state: State<'_, AppState>,
    serial: String,
    local: String,
    remote: String,
) -> AppResult<()> {
    state.adb.upload(&serial, &local, &remote).await
}

#[tauri::command]
pub async fn files_upload_bytes(
    state: State<'_, AppState>,
    serial: String,
    remote_dir: String,
    name: String,
    data: String,
) -> AppResult<()> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())))?;
    state.adb.upload_bytes(&serial, &remote_dir, &name, bytes).await
}

#[tauri::command]
pub async fn files_download(
    state: State<'_, AppState>,
    serial: String,
    remote: String,
    local: String,
) -> AppResult<()> {
    // 相对文件名默认保存到用户下载目录
    let is_bare = !local.contains('/') && !local.contains('\\');
    let target = if is_bare {
        crate::util::downloads_dir()?
            .join(&local)
            .to_string_lossy()
            .into_owned()
    } else {
        local
    };
    state.adb.download(&serial, &remote, &target).await
}

#[tauri::command]
pub async fn files_read(
    state: State<'_, AppState>,
    serial: String,
    path: String,
    max: Option<usize>,
) -> AppResult<String> {
    state.adb.read_preview(&serial, &path, max.unwrap_or(1024 * 512)).await
}

// ────────────────────────── 应用管理 ──────────────────────────

#[tauri::command]
pub async fn apps_list(
    state: State<'_, AppState>,
    serial: String,
    include_system: bool,
) -> AppResult<Vec<crate::adb::apps::AppEntry>> {
    state.adb.list_apps(&serial, include_system).await
}

#[tauri::command]
pub async fn apps_install(state: State<'_, AppState>, serial: String, local_apk: String) -> AppResult<()> {
    state.adb.install_apk(&serial, &local_apk).await
}

#[tauri::command]
pub async fn apps_install_bytes(
    state: State<'_, AppState>,
    serial: String,
    name: String,
    data: String,
) -> AppResult<()> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())))?;
    state.adb.install_apk_bytes(&serial, &name, bytes).await
}

#[tauri::command]
pub async fn apps_uninstall(
    state: State<'_, AppState>,
    serial: String,
    package: String,
    keep_data: bool,
) -> AppResult<()> {
    state.adb.uninstall_app(&serial, &package, keep_data).await
}

#[tauri::command]
pub async fn apps_launch(state: State<'_, AppState>, serial: String, package: String) -> AppResult<()> {
    state.adb.launch_app(&serial, &package).await
}

#[tauri::command]
pub async fn apps_stop(state: State<'_, AppState>, serial: String, package: String) -> AppResult<()> {
    state.adb.stop_app(&serial, &package).await
}

#[tauri::command]
pub async fn apps_info(
    state: State<'_, AppState>,
    serial: String,
    package: String,
) -> AppResult<crate::adb::apps::AppDetail> {
    state.adb.app_detail(&serial, &package).await
}

// ────────────────────────── 快捷指令 ──────────────────────────

#[tauri::command]
pub async fn action_screenshot(state: State<'_, AppState>, serial: String) -> AppResult<String> {
    state.adb.screenshot(&serial).await
}

#[tauri::command]
pub async fn action_screenshot_save(state: State<'_, AppState>, serial: String) -> AppResult<String> {
    state.adb.screenshot_save(&serial).await
}

#[tauri::command]
pub async fn action_record(state: State<'_, AppState>, serial: String, seconds: u32) -> AppResult<String> {
    state.adb.record(&serial, seconds).await
}

#[tauri::command]
pub async fn action_key(
    state: State<'_, AppState>,
    serial: String,
    keycode: u16,
) -> AppResult<()> {
    if key_via_socket(&state, &serial, keycode).await? {
        return Ok(());
    }
    state.input.keyevent(&state.adb, &serial, keycode).await
}

// ────────────────────────── 宏 ──────────────────────────

#[tauri::command]
pub async fn macro_save(
    state: State<'_, AppState>,
    name: String,
    steps: Vec<MacroStepAt>,
    screen_size: Option<(u32, u32)>,
) -> AppResult<String> {
    let m = Macro::new(name, steps, screen_size);
    crate::macros::save(&state.config_dir, &m)?;
    state.macros.lock().await.push(m.clone());
    Ok(m.id)
}

#[tauri::command]
pub async fn macro_list(state: State<'_, AppState>) -> AppResult<Vec<Macro>> {
    Ok(state.macros.lock().await.clone())
}

#[tauri::command]
pub async fn macro_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    crate::macros::delete(&state.config_dir, &id)?;
    state.macros.lock().await.retain(|m| m.id != id);
    Ok(())
}

#[tauri::command]
pub async fn macro_play(
    state: State<'_, AppState>,
    serial: String,
    macro_id: String,
    on_progress: Channel<u32>,
) -> AppResult<()> {
    let m = state
        .macros
        .lock()
        .await
        .iter()
        .find(|m| m.id == macro_id)
        .cloned()
        .ok_or_else(|| AppError::Macro("宏不存在".into()))?;

    // 回放前校验屏幕尺寸（不一致仅警告，不阻止）
    if let Some((w, h)) = m.screen_size {
        if let Ok(screen) = state.adb.wm_size(&serial).await {
            if screen != (w, h) {
                eprintln!(
                    "[phonectrl] 警告: 宏录制分辨率 {w}x{h} 与当前 {screen:?} 不一致"
                );
            }
        }
    }

    let mut prev = 0u64;
    for (i, step_at) in m.steps.iter().enumerate() {
        let delta = step_at.ts.saturating_sub(prev).clamp(0, 3000);
        prev = step_at.ts;
        if delta > 0 {
            tokio::time::sleep(Duration::from_millis(delta)).await;
        }
        exec_macro_step(&state, &serial, &step_at.step).await?;
        let _ = on_progress.send((i + 1) as u32);
    }
    Ok(())
}

async fn exec_macro_step(state: &AppState, serial: &str, step: &MacroStep) -> AppResult<()> {
    match step {
        MacroStep::Tap { x, y } => {
            if !touch_via_socket(state, serial, &[(ACTION_DOWN, *x, *y), (ACTION_UP, *x, *y)])
                .await?
            {
                let (dx, dy) = device_coords(state, serial, *x, *y).await;
                state.input.tap(&state.adb, serial, dx, dy).await?;
            }
            Ok(())
        }
        MacroStep::Swipe {
            x1,
            y1,
            x2,
            y2,
            duration_ms,
        } => {
            if !touch_via_socket(
                state,
                serial,
                &[
                    (ACTION_DOWN, *x1, *y1),
                    (ACTION_MOVE, (x1 + x2) / 2, (y1 + y2) / 2),
                    (ACTION_UP, *x2, *y2),
                ],
            )
            .await?
            {
                let (dx1, dy1) = device_coords(state, serial, *x1, *y1).await;
                let (dx2, dy2) = device_coords(state, serial, *x2, *y2).await;
                state
                    .input
                    .swipe(&state.adb, serial, dx1, dy1, dx2, dy2, *duration_ms)
                    .await?;
            }
            Ok(())
        }
        MacroStep::Key { keycode } => {
            if !key_via_socket(state, serial, *keycode).await? {
                state.input.keyevent(&state.adb, serial, *keycode).await?;
            }
            Ok(())
        }
        MacroStep::Text { text } => {
            if !text_via_socket(state, serial, text).await? {
                state.input.text(&state.adb, serial, text).await?;
            }
            Ok(())
        }
        MacroStep::Wait { ms } => {
            tokio::time::sleep(Duration::from_millis(*ms)).await;
            Ok(())
        }
        MacroStep::Screenshot => {
            state.adb.screenshot(serial).await.map(|_| ())
        }
        MacroStep::Home => {
            if !key_via_socket(state, serial, crate::adb::input::KEYCODE_HOME).await? {
                state.input.keyevent(&state.adb, serial, crate::adb::input::KEYCODE_HOME).await?;
            }
            Ok(())
        }
        MacroStep::Back => {
            if !key_via_socket(state, serial, crate::adb::input::KEYCODE_BACK).await? {
                state.input.keyevent(&state.adb, serial, crate::adb::input::KEYCODE_BACK).await?;
            }
            Ok(())
        }
        MacroStep::Recents => {
            if !key_via_socket(state, serial, crate::adb::input::KEYCODE_APP_SWITCH).await? {
                state.input.keyevent(&state.adb, serial, crate::adb::input::KEYCODE_APP_SWITCH).await?;
            }
            Ok(())
        }
    }
}
