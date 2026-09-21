use crate::adb::{input::InputQueue, AdbClient};
use crate::config::AppConfig;
use crate::macros::Macro;
use crate::stream::StreamHandle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// 全局应用状态（托管为 Tauri State<AppState>，Clone 以便后台任务共享）。
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub adb: AdbClient,
    pub ffmpeg_path: Arc<RwLock<PathBuf>>,
    pub streams: Arc<Mutex<HashMap<String, Arc<StreamHandle>>>>,
    pub input: InputQueue,
    pub macros: Arc<Mutex<Vec<Macro>>>,
    pub config_dir: PathBuf,
    /// 无控制通道时 input_touch 的起点记录（adb 回退聚合）
    pub touch_origins: Arc<Mutex<HashMap<String, (u32, u32)>>>,
}

impl AppState {
    pub async fn new(config_dir: PathBuf) -> Self {
        let cfg = crate::config::load(&config_dir);
        let (adb_path, ffmpeg_path) = crate::adb::resolve_paths(&cfg).unwrap_or_else(|_| {
            // 探测失败时使用占位路径，运行时在命令中报具体错误
            (
                PathBuf::from("adb"),
                PathBuf::from("ffmpeg"),
            )
        });
        let adb = AdbClient::new(adb_path);
        let macros = crate::macros::load_all(&config_dir);
        Self {
            config: Arc::new(RwLock::new(cfg)),
            adb,
            ffmpeg_path: Arc::new(RwLock::new(ffmpeg_path)),
            streams: Arc::new(Mutex::new(HashMap::new())),
            input: InputQueue::default(),
            macros: Arc::new(Mutex::new(macros)),
            config_dir,
            touch_origins: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
