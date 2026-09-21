use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MacroStep {
    Tap { x: u32, y: u32 },
    #[serde(rename_all = "camelCase")]
    Swipe {
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    },
    Key { keycode: u16 },
    Text { text: String },
    Wait { ms: u64 },
    Screenshot,
    Home,
    Back,
    Recents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroStepAt {
    /// 相对录制开始时间的毫秒偏移
    pub ts: u64,
    pub step: MacroStep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Macro {
    pub id: String,
    pub name: String,
    pub steps: Vec<MacroStepAt>,
    pub created_at: u64,
    /// 录制时的设备屏幕尺寸（回放前校验用）
    pub screen_size: Option<(u32, u32)>,
}

impl Macro {
    pub fn new(name: String, steps: Vec<MacroStepAt>, screen_size: Option<(u32, u32)>) -> Self {
        Self {
            id: Uuid::new_v4().simple().to_string(),
            name,
            steps,
            created_at: crate::util::now_ms(),
            screen_size,
        }
    }
}

pub fn load_all(config_dir: &std::path::Path) -> Vec<Macro> {
    let dir = config_dir.join("macros");
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(m) = serde_json::from_str::<Macro>(&s) {
                    out.push(m);
                }
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

pub fn save(config_dir: &std::path::Path, m: &Macro) -> AppResult<()> {
    let dir = config_dir.join("macros");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", m.id));
    let s = serde_json::to_string_pretty(m).map_err(|e| AppError::Macro(e.to_string()))?;
    std::fs::write(path, s)?;
    Ok(())
}

pub fn delete(config_dir: &std::path::Path, id: &str) -> AppResult<()> {
    let path = config_dir.join("macros").join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
