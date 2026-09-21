use super::AdbClient;
use crate::error::AppResult;
use crate::util::shell_quote;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 输入命令串行执行器：所有 input 命令共用一把异步锁，
/// 避免宏回放与手动操作并发导致 adb 调用错乱。
#[derive(Clone, Default)]
pub struct InputQueue {
    inner: Arc<Mutex<()>>,
}

impl InputQueue {
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.inner.lock().await
    }

    pub async fn tap(&self, adb: &AdbClient, serial: &str, x: u32, y: u32) -> AppResult<()> {
        let _g = self.lock().await;
        adb.run(Some(serial), &["shell", "input", "tap", &x.to_string(), &y.to_string()])
            .await?
            .ok()
    }

    pub async fn swipe(
        &self,
        adb: &AdbClient,
        serial: &str,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> AppResult<()> {
        let _g = self.lock().await;
        adb.run(
            Some(serial),
            &[
                "shell", "input", "swipe", &x1.to_string(), &y1.to_string(), &x2.to_string(),
                &y2.to_string(), &duration_ms.to_string(),
            ],
        )
        .await?
        .ok()
    }

    pub async fn keyevent(&self, adb: &AdbClient, serial: &str, keycode: u16) -> AppResult<()> {
        let _g = self.lock().await;
        adb.run(
            Some(serial),
            &["shell", "input", "keyevent", &keycode.to_string()],
        )
        .await?
        .ok()
    }

    /// ASCII 文本输入（空格转 %s）。
    pub async fn text(&self, adb: &AdbClient, serial: &str, text: &str) -> AppResult<()> {
        let _g = self.lock().await;
        let escaped = text.replace(' ', "%s");
        let quoted = shell_quote(&escaped);
        adb.run(Some(serial), &["shell", "input", "text", &quoted]).await?.ok()
    }

    /// 非 ASCII 文本：写入剪贴板后触发粘贴（Android 10+）。
    pub async fn text_unicode(
        &self,
        adb: &AdbClient,
        serial: &str,
        text: &str,
    ) -> AppResult<()> {
        let _g = self.lock().await;
        let quoted = shell_quote(text);
        adb.run(
            Some(serial),
            &["shell", "cmd", "clipboard", "set-text", &quoted],
        )
        .await?
        .ok()?;
        adb.run(Some(serial), &["shell", "input", "keyevent", "279"]).await?.ok()
    }
}

pub const KEYCODE_HOME: u16 = 3;
pub const KEYCODE_BACK: u16 = 4;
pub const KEYCODE_APP_SWITCH: u16 = 187;
