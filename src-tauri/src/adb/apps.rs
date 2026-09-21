use super::AdbClient;
use crate::error::{AppError, AppResult};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub package: String,
    pub version: Option<String>,
    pub system: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDetail {
    pub package: String,
    pub version: Option<String>,
    pub uid: Option<String>,
    pub system: bool,
    pub enabled: bool,
    pub label: Option<String>,
}

impl AdbClient {
    /// 列出已安装应用（默认仅三方应用）。
    pub async fn list_apps(&self, serial: &str, include_system: bool) -> AppResult<Vec<AppEntry>> {
        let flag = if include_system { "-f" } else { "-3" };
        let out = self
            .run(Some(serial), &["shell", "pm", "list", "packages", flag])
            .await?;
        if out.code != 0 {
            return Err(AppError::Adb(format!("无法列出应用: {}", out.stderr.trim())));
        }
        let mut apps = Vec::new();
        for line in out.stdout.lines() {
            if let Some(pkg) = line.trim().strip_prefix("package:") {
                apps.push(AppEntry {
                    package: pkg.to_string(),
                    version: None,
                    system: include_system,
                    enabled: true,
                });
            }
        }
        Ok(apps)
    }

    pub async fn install_apk(&self, serial: &str, local_apk: &str) -> AppResult<()> {
        self.ensure_server().await?;
        let out = self
            .run(Some(serial), &["install", "-r", local_apk])
            .await?;
        if !out.stdout.contains("Success") {
            return Err(AppError::Adb(format!(
                "安装失败: {}",
                if out.stderr.trim().is_empty() {
                    out.stdout.trim()
                } else {
                    out.stderr.trim()
                }
            )));
        }
        Ok(())
    }

    /// 以字节安装：前端选择 APK 后经 IPC 传内容，落临时文件再 adb install。
    pub async fn install_apk_bytes(
        &self,
        serial: &str,
        name: &str,
        data: Vec<u8>,
    ) -> AppResult<()> {
        let tmp = std::env::temp_dir().join(format!(
            "phonectrl_apk_{}_{}",
            uuid::Uuid::new_v4().simple(),
            name
        ));
        std::fs::write(&tmp, data)?;
        let res = self.install_apk(serial, &tmp.to_string_lossy()).await;
        let _ = std::fs::remove_file(&tmp);
        res
    }

    pub async fn uninstall_app(&self, serial: &str, package: &str, keep_data: bool) -> AppResult<()> {
        self.ensure_server().await?;
        let flag = if keep_data { "-k" } else { "" };
        let out = self
            .run(Some(serial), &["uninstall", flag, package])
            .await?;
        if !out.stdout.contains("Success") {
            return Err(AppError::Adb(format!("卸载失败: {}", out.stderr.trim())));
        }
        Ok(())
    }

    /// 启动应用主 Activity（老系统回退 monkey）。
    pub async fn launch_app(&self, serial: &str, package: &str) -> AppResult<()> {
        let out = self
            .run(
                Some(serial),
                &["shell", "cmd", "package", "resolve-activity", "--brief", package],
            )
            .await?;
        let activity = out
            .stdout
            .lines()
            .nth(1)
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty());
        match activity {
            Some(act) => {
                let q = crate::util::shell_quote(&act);
                self.run(Some(serial), &["shell", "am", "start", "-n", &q]).await?.ok()
            }
            None => {
                let qp = crate::util::shell_quote(package);
                self.run(Some(serial), &["shell", "monkey", "-p", &qp, "1"]).await?.ok()
            }
        }
    }

    pub async fn stop_app(&self, serial: &str, package: &str) -> AppResult<()> {
        let q = crate::util::shell_quote(package);
        self.run(Some(serial), &["shell", "am", "force-stop", &q]).await?.ok()
    }

    pub async fn app_detail(&self, serial: &str, package: &str) -> AppResult<AppDetail> {
        let out = self
            .run(Some(serial), &["shell", "dumpsys", "package", package])
            .await?;
        if out.code != 0 {
            return Err(AppError::Adb(format!("获取应用信息失败: {}", out.stderr.trim())));
        }
        let version = out
            .stdout
            .lines()
            .find_map(|l| l.trim().strip_prefix("versionName=").map(|v| v.to_string()));
        let uid = out
            .stdout
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("userId=")
                    .map(|v| v.trim_end_matches('s').to_string())
            });
        Ok(AppDetail {
            package: package.to_string(),
            version,
            uid,
            system: false,
            enabled: true,
            label: None,
        })
    }
}
