use super::AdbClient;
use crate::error::{AppError, AppResult};
use base64::Engine;
use uuid::Uuid;

impl AdbClient {
    /// 截屏并返回 PNG 的 base64 字符串。
    pub async fn screenshot(&self, serial: &str) -> AppResult<String> {
        let data = self
            .run_raw(Some(serial), &["exec-out", "screencap", "-p"])
            .await?;
        if data.is_empty() {
            return Err(AppError::Adb("截屏失败：返回空数据".into()));
        }
        Ok(base64::engine::general_purpose::STANDARD.encode(&data))
    }

    /// 截屏并保存到本地 Downloads 目录，返回本地路径。
    pub async fn screenshot_save(&self, serial: &str) -> AppResult<String> {
        let data = self
            .run_raw(Some(serial), &["exec-out", "screencap", "-p"])
            .await?;
        if data.is_empty() {
            return Err(AppError::Adb("截屏失败：返回空数据".into()));
        }
        let dir = crate::util::downloads_dir()?;
        let path = dir.join(format!("phonectrl_shot_{}.png", Uuid::new_v4().simple()));
        std::fs::write(&path, &data)?;
        Ok(path.to_string_lossy().into_owned())
    }

    /// 录屏：设备端成片后 pull 到本地 Downloads 并删除设备端文件。
    pub async fn record(&self, serial: &str, seconds: u32) -> AppResult<String> {
        let remote = format!("/sdcard/phonectrl_{}.mp4", Uuid::new_v4().simple());
        self.run(
            Some(serial),
            &[
                "shell", "screenrecord", "--time-limit", &seconds.to_string(), &remote,
            ],
        )
        .await?
        .ok()?;

        let dir = crate::util::downloads_dir()?;
        let local = dir.join(format!("phonectrl_rec_{}.mp4", Uuid::new_v4().simple()));
        let local_str = local.to_string_lossy().into_owned();
        self.ensure_server().await?;
        let out = self
            .run(Some(serial), &["pull", &remote, &local_str])
            .await?;
        // 清理设备端临时文件（失败忽略）
        let _ = self.run(Some(serial), &["shell", "rm", "-f", &remote]).await;
        if out.code != 0 {
            return Err(AppError::Adb(format!("录屏文件拉取失败: {}", out.stderr.trim())));
        }
        Ok(local_str)
    }
}
