use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("ADB 执行失败: {0}")]
    Adb(String),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("设备错误: {0}")]
    Device(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("投屏流错误: {0}")]
    Stream(String),
    #[error("宏错误: {0}")]
    Macro(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<tokio::sync::watch::error::RecvError> for AppError {
    fn from(_: tokio::sync::watch::error::RecvError) -> Self {
        AppError::Stream("帧通道已关闭".into())
    }
}
