//! motionevent 流式注入通道（持久 adb shell + `input motionevent`）。
//!
//! 背景：sendevent 注入桥需要写 /dev/input（SELinux 在 EMUI 等系统上拒绝
//! shell 域写入，桥不可用）；而聚合通道只在抬手时重放 `input swipe`，
//! 实测 EMUI 的 `input swipe` 注入事件速度信息不产生惯性滚动（快速轻扫
//! 松手后列表戛然而止），且拖动过程完全不跟随鼠标。
//!
//! `input motionevent DOWN|MOVE|UP` 是框架级注入（与 scrcpy 同机制），
//! 在这些机型上实测可用，且设备端 VelocityTracker 能从流式事件中算出
//! 真实甩动速度 → 抬手后由应用自身物理产生与真手指一致的惯性。
//!
//! 实现：每台设备常驻一条 `adb shell` 会话，写入线程从通道取事件逐条
//! 写入 stdin。设备端按自身节奏（实测 ~18ms/条）顺序 fork 注入，因此：
//!   - 实时事件采用「最新位置合并」：爆发时只保留首个与最新位置，防止
//!     队列积压导致画面橡皮筋式滞后；
//!   - 脚本化手势（swipe 重放）走 Seq 原样写入，由设备端均匀插值出
//!     接近真实时长的连续轨迹，保留甩动速度。
//!
//! 通道优先级（commands.rs）：注入桥 > scrcpy 控制通道(显式) > 本通道 > adb 聚合。

use crate::adb::AdbClient;
use crate::error::AppResult;
use crate::stream::{ACTION_DOWN, ACTION_MOVE, ACTION_UP};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;

/// 注入消息：Ev = 实时事件（move 合并），Seq = 原样顺序写入的手势序列。
pub enum Msg {
    Ev(u8, u32, u32),
    Seq(Vec<(u8, u32, u32)>),
}

#[derive(Clone)]
pub struct MotionShellHandle {
    tx: Sender<Msg>,
}

impl MotionShellHandle {
    /// 发送单条实时事件。返回 false 表示通道已关闭（写入线程退出）。
    pub fn send(&self, action: u8, x: u32, y: u32) -> bool {
        self.tx.send(Msg::Ev(action, x, y)).is_ok()
    }

    /// 原样写入一段事件序列（脚本化手势，不做合并）。
    pub fn send_seq(&self, events: Vec<(u8, u32, u32)>) -> bool {
        self.tx.send(Msg::Seq(events)).is_ok()
    }
}

/// 设备级 motionevent 注入通道管理器（挂 AppState，全局共享）。
#[derive(Default)]
pub struct MotionManager {
    /// 串号 → 活跃通道（tx 丢弃即令写入线程退出并杀掉 shell 子进程）。
    shells: Mutex<HashMap<String, MotionShellHandle>>,
    /// 探测失败的设备（`input motionevent` 不存在），本会话内不再重试。
    probe_failed: Mutex<HashSet<String>>,
}

impl MotionManager {
    /// 取该设备的注入通道；不存在则探测并懒启动。探测失败返回 None。
    pub async fn ensure(&self, adb: &AdbClient, serial: &str) -> Option<MotionShellHandle> {
        {
            let shells = self.shells.lock().unwrap();
            if let Some(h) = shells.get(serial) {
                // 通道是否已死由调用方 send 失败时判定并清理（std Sender
                // 没有 is_closed，send 返回 Err 即通道关闭）。
                return Some(h.clone());
            }
        }
        if self.probe_failed.lock().unwrap().contains(serial) {
            return None;
        }

        // 探测 `input motionevent` 子命令（Android 12+ / 部分厂商定制系统可用）
        let probe = match adb.run(Some(serial), &["shell", "input"]).await {
            Ok(o) => format!("{}{}", o.stdout, o.stderr).to_lowercase(),
            Err(_) => String::new(),
        };
        if !probe.contains("motionevent") {
            self.probe_failed.lock().unwrap().insert(serial.into());
            return None;
        }

        match self.spawn(adb, serial).await {
            Ok(h) => {
                self.shells
                    .lock()
                    .unwrap()
                    .insert(serial.into(), h.clone());
                Some(h)
            }
            Err(e) => {
                eprintln!("[phonectrl] motionevent 通道启动失败({serial}): {e}");
                self.probe_failed.lock().unwrap().insert(serial.into());
                None
            }
        }
    }

    async fn spawn(&self, adb: &AdbClient, serial: &str) -> AppResult<MotionShellHandle> {
        let adb_path = adb.path().await;
        let mut child = Command::new(&adb_path)
            .args(["-s", serial, "shell"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| crate::error::AppError::Adb(format!("adb shell 启动失败: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| crate::error::AppError::Adb("adb shell 无 stdin".into()))?;

        let (tx, rx) = std::sync::mpsc::channel::<Msg>();
        // 写入线程持有子进程：退出时杀掉，避免孤儿 adb shell。
        std::thread::spawn(move || writer_loop(stdin, rx, child));
        Ok(MotionShellHandle { tx })
    }

    /// 停止设备通道（设备断开/流停止时调用；tx 丢弃即触发写入线程退出）。
    pub fn stop(&self, serial: &str) {
        self.shells.lock().unwrap().remove(serial);
    }
}

fn action_name(a: u8) -> &'static str {
    match a {
        ACTION_DOWN => "DOWN",
        ACTION_UP => "UP",
        ACTION_MOVE => "MOVE",
        _ => "MOVE",
    }
}

fn write_ev(stdin: &mut ChildStdin, action: u8, x: u32, y: u32) -> std::io::Result<()> {
    // 逐条写 + flush：立即进入 adb 传输，设备端按自身节奏顺序执行。
    stdin.write_all(format!("input motionevent {} {} {}\n", action_name(action), x, y).as_bytes())?;
    stdin.flush()
}

/// 写入线程主循环：合并爆发 move、顺序写入手势序列、退出时清理子进程。
fn writer_loop(mut stdin: ChildStdin, rx: Receiver<Msg>, mut child: Child) {
    let mut pending: Option<(u32, u32)> = None;
    let mut dead = false;

    while let Ok(msg) = rx.recv() {
        let mut batch = vec![msg];
        // 立即排空积压：爆发合并到一个批次内处理
        while let Ok(m) = rx.try_recv() {
            batch.push(m);
        }
        for m in batch {
            match m {
                Msg::Seq(events) => {
                    if let Some((x, y)) = pending.take() {
                        if write_ev(&mut stdin, ACTION_MOVE, x, y).is_err() {
                            dead = true;
                            break;
                        }
                    }
                    for (a, x, y) in events {
                        if write_ev(&mut stdin, a, x, y).is_err() {
                            dead = true;
                            break;
                        }
                    }
                }
                Msg::Ev(ACTION_MOVE, x, y) => pending = Some((x, y)),
                Msg::Ev(ACTION_DOWN, x, y) => {
                    // 新手势开始：丢弃上个手势残留的待发 move
                    pending = None;
                    if write_ev(&mut stdin, ACTION_DOWN, x, y).is_err() {
                        dead = true;
                    }
                }
                Msg::Ev(ACTION_UP, x, y) => {
                    // 先补发最新位置，再在目标点抬起，保持速度轨迹连续
                    if let Some((px, py)) = pending.take() {
                        if write_ev(&mut stdin, ACTION_MOVE, px, py).is_err() {
                            dead = true;
                            break;
                        }
                    }
                    if write_ev(&mut stdin, ACTION_UP, x, y).is_err() {
                        dead = true;
                    }
                }
                Msg::Ev(_, x, y) => {
                    if write_ev(&mut stdin, ACTION_MOVE, x, y).is_err() {
                        dead = true;
                    }
                }
            }
            if dead {
                break;
            }
        }
        if dead {
            break;
        }
        // 批次处理完，冲刷待发 move（保持事件尽快下发）
        if let Some((x, y)) = pending.take() {
            if write_ev(&mut stdin, ACTION_MOVE, x, y).is_err() {
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// 生成 swipe 的插值事件序列（DOWN + 均匀 MOVE + UP）。
/// 设备端按 ~18ms/条 的节奏顺序注入，总时长≈条数×节奏，故步数按
/// duration/18 估算，让设备端重放出接近真实时长的连续轨迹并保留速度。
pub fn swipe_events(
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    duration_ms: u32,
) -> Vec<(u8, u32, u32)> {
    let steps = (duration_ms as f32 / 18.0).round().clamp(2.0, 60.0) as u32;
    let mut events = Vec::with_capacity(steps as usize + 2);
    events.push((ACTION_DOWN, x1, y1));
    for i in 1..steps {
        let t = i as f32 / steps as f32;
        let x = (x1 as f32 + (x2 as f32 - x1 as f32) * t).round() as u32;
        let y = (y1 as f32 + (y2 as f32 - y1 as f32) * t).round() as u32;
        events.push((ACTION_MOVE, x, y));
    }
    events.push((ACTION_UP, x2, y2));
    events
}
