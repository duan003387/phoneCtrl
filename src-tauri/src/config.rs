use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    /// 用户手动指定的 adb 路径（可空）
    pub adb_path: Option<String>,
    /// 用户手动指定的 ffmpeg 路径（可空）
    pub ffmpeg_path: Option<String>,
    /// 投屏比特率
    pub stream_bitrate: u32,
    /// 投屏最大宽度（等比缩放）
    pub stream_max_width: u32,
    /// 投屏目标帧率
    pub stream_fps: u32,
    /// MJPEG 质量 (ffmpeg -q:v)
    pub stream_quality: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            adb_path: None,
            ffmpeg_path: None,
            stream_bitrate: 12_000_000,
            stream_max_width: 720,
            stream_fps: 60,
            stream_quality: 10,
        }
    }
}

pub fn config_file(config_dir: &std::path::Path) -> PathBuf {
    config_dir.join("config.json")
}

pub fn load(config_dir: &std::path::Path) -> AppConfig {
    match std::fs::read_to_string(config_file(config_dir)) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save(config_dir: &std::path::Path, cfg: &AppConfig) -> AppResult<()> {
    let dir = config_dir;
    std::fs::create_dir_all(dir)?;
    let s = serde_json::to_string_pretty(cfg).map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(config_file(dir), s)?;
    Ok(())
}

/// 探测 adb 可执行文件路径。
pub fn detect_adb(configured: Option<&str>) -> AppResult<PathBuf> {
    if let Some(p) = configured {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Ok(pb);
        }
        return Err(AppError::Config(format!("配置的 adb 路径不存在: {p}")));
    }

    // 环境变量 ANDROID_HOME / ANDROID_SDK_ROOT
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(home) = std::env::var(var) {
            let pb = PathBuf::from(&home).join("platform-tools").join(adb_exe());
            if pb.exists() {
                return Ok(pb);
            }
        }
    }

    // 常见位置
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let pb = PathBuf::from(home)
                .join("Library/Android/sdk/platform-tools")
                .join(adb_exe());
            if pb.exists() {
                return Ok(pb);
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let pb = PathBuf::from(local)
                .join("Android/Sdk/platform-tools")
                .join(adb_exe());
            if pb.exists() {
                return Ok(pb);
            }
        }
    }

    // which adb
    if let Some(p) = which("adb") {
        return Ok(p);
    }

    Err(AppError::Config(
        "未找到 adb，请在设置中手动指定路径（Android SDK platform-tools 下的 adb）。".into(),
    ))
}

/// 探测 ffmpeg 可执行文件路径。
pub fn detect_ffmpeg(configured: Option<&str>) -> AppResult<PathBuf> {
    if let Some(p) = configured {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Ok(pb);
        }
        return Err(AppError::Config(format!("配置的 ffmpeg 路径不存在: {p}")));
    }

    if let Some(p) = which("ffmpeg") {
        return Ok(p);
    }

    #[cfg(target_os = "macos")]
    {
        let pb = PathBuf::from("/opt/homebrew/bin/ffmpeg");
        if pb.exists() {
            return Ok(pb);
        }
    }

    Err(AppError::Config(
        "未找到 ffmpeg，请在设置中手动指定路径。".into(),
    ))
}

fn adb_exe() -> &'static str {
    if cfg!(target_os = "windows") {
        "adb.exe"
    } else {
        "adb"
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(name);
        if full.exists() {
            return Some(full);
        }
        #[cfg(target_os = "windows")]
        {
            let full_exe = dir.join(format!("{name}.exe"));
            if full_exe.exists() {
                return Some(full_exe);
            }
        }
    }
    None
}
