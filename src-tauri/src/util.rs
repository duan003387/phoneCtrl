/// mksh 风格单引号转义，用于安全地把参数嵌入 adb shell 命令字符串。
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// toybox `ls -la` 输出解析（已知限制：文件名含空格会解析错误）。
/// 字段: 权限 硬链 owner group 大小 日期 时间 名称
pub fn parse_ls_line(line: &str) -> Option<(String, u64, bool, String)> {
    if line.is_empty() || line.starts_with("total ") {
        return None;
    }
    let mut it = line.split_whitespace();
    let perm = it.next()?;
    if perm.len() < 2 || !perm.starts_with('-') && !perm.starts_with('d') && !perm.starts_with('l') {
        return None;
    }
    let _links = it.next()?;
    let _owner = it.next()?;
    let _group = it.next()?;
    let size: u64 = it.next()?.parse().ok()?;
    let _date = it.next()?;
    let _time = it.next()?;
    let name = it.collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return None;
    }
    let is_dir = perm.starts_with('d') || perm.starts_with('l');
    Some((name, size, is_dir, perm.to_string()))
}

/// 解析 `adb devices -l` 输出中的一行。
pub fn parse_device_line(line: &str) -> Option<(String, String, Option<String>, Option<String>)> {
    let mut it = line.split_whitespace();
    let serial = it.next()?;
    let state = it.next()?;
    let mut model = None;
    let mut product = None;
    for attr in it {
        if let Some(v) = attr.strip_prefix("model:") {
            model = Some(v.to_string());
        } else if let Some(v) = attr.strip_prefix("product:") {
            product = Some(v.to_string());
        }
    }
    Some((serial.to_string(), state.to_string(), model, product))
}

/// 计算流分辨率：按 max_width 等比缩放后向上取偶（H.264 要求宽高为偶数）。
pub fn scaled_stream_size(w: u32, h: u32, max_width: u32) -> (u32, u32) {
    if w <= max_width {
        return (even(w), even(h));
    }
    let sw = max_width;
    let sh = (h * max_width + w / 2) / w;
    (even(sw), even(sh))
}

fn even(v: u32) -> u32 {
    if v % 2 == 0 {
        v
    } else {
        v + 1
    }
}

/// 当前 Unix 时间戳（毫秒）。
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 用户下载目录（不存在则创建）。
pub fn downloads_dir() -> Result<std::path::PathBuf, std::io::Error> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "HOME 未设置")
    })?;
    let dir = std::path::PathBuf::from(home).join("Downloads");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Tauri 资源目录（macOS 打包后为 Contents/Resources）。在 run() 的 setup 里设置一次。
static RESOURCE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub fn set_resource_dir(p: std::path::PathBuf) {
    let _ = RESOURCE_DIR.set(p);
}

/// 解析随 app 分发的资源文件（相对 resources 根，可含子目录如 `adb/adb`）。
/// 依次尝试：dev 的 CARGO_MANIFEST_DIR/resources → Tauri resource_dir（打包目录）
/// → 可执行文件同级。返回存在的第一个。
pub fn resolve_resource(rel: &str) -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = std::path::PathBuf::from(dir).join("resources").join(rel);
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(rd) = RESOURCE_DIR.get() {
        let p = rd.join(rel);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join(rel);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

/// 确保可执行文件带执行位（打包/解压可能丢失权限）。仅 unix 生效。
#[cfg(unix)]
pub fn ensure_executable(path: &std::path::Path) {
    if let Ok(md) = std::fs::metadata(path) {
        let mut perms = md.permissions();
        use std::os::unix::fs::PermissionsExt;
        if perms.mode() & 0o111 == 0 {
            perms.set_mode(perms.mode() | 0o755);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

#[cfg(not(unix))]
pub fn ensure_executable(_path: &std::path::Path) {}

// ────────────────────────── Windows 子进程控制台抑制 ──────────────────────────
//
// release 构建里 main.rs 用 `windows_subsystem = "windows"`，进程没有附着控制台；
// 此时每拉起一个控制台子系统子进程（adb.exe 是），Windows 会给它新建一个控制台
// 窗口。adb 属于轮询级高频调用（设备列表/属性/输入回退），不加这个标志就是满屏闪 cmd 窗口。
// CREATE_NO_WINDOW = 0x0800_0000。dev 构建附着了父控制台所以看不出来，只在安装包上复现。

/// std::process::Command 版本。
#[cfg(windows)]
pub fn hide_console(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}

/// tokio::process::Command 版本。
#[cfg(windows)]
pub fn hide_console_async(cmd: &mut tokio::process::Command) {
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
pub fn hide_console(_cmd: &mut std::process::Command) {}

#[cfg(not(windows))]
pub fn hide_console_async(_cmd: &mut tokio::process::Command) {}
