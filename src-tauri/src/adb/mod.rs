pub mod apps;
pub mod devices;
pub mod files;
pub mod input;
pub mod media;

use crate::config;
use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use tokio::sync::RwLock;

/// 设备/工具路径（可运行时更新）。
#[derive(Clone)]
pub struct AdbClient {
    path: std::sync::Arc<RwLock<PathBuf>>,
}

#[derive(Debug, Clone)]
pub struct AdbOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl AdbOutput {
    pub fn ok(&self) -> AppResult<()> {
        if self.code == 0 {
            Ok(())
        } else {
            Err(AppError::Adb(format!(
                "退出码 {}: {}",
                self.code,
                self.stderr.trim()
            )))
        }
    }
}

impl AdbClient {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path: std::sync::Arc::new(RwLock::new(path)),
        }
    }

    pub async fn path(&self) -> PathBuf {
        self.path.read().await.clone()
    }

    pub async fn set_path(&self, path: PathBuf) {
        *self.path.write().await = path;
    }

    /// 通用执行：`adb [-s serial] args`，捕获输出。
    pub async fn run(&self, serial: Option<&str>, args: &[&str]) -> AppResult<AdbOutput> {
        let adb = self.path().await;
        let mut cmd_args: Vec<String> = Vec::new();
        if let Some(s) = serial {
            cmd_args.push("-s".into());
            cmd_args.push(s.into());
        }
        cmd_args.extend(args.iter().map(|a| a.to_string()));
        let adb2 = adb.clone();
        let output = tauri::async_runtime::spawn_blocking(move || {
            let mut c = Command::new(&adb2);
            c.args(&cmd_args);
            crate::util::hide_console(&mut c);
            c.output()
        })
        .await
        .map_err(|e| AppError::Adb(e.to_string()))?
        .map_err(|e| AppError::Adb(format!("启动 adb 失败 ({adb:?}): {e}")))?;

        Ok(AdbOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code().unwrap_or(-1),
        })
    }

    /// 原始二进制执行（exec-out），返回字节。
    pub async fn run_raw(&self, serial: Option<&str>, args: &[&str]) -> AppResult<Vec<u8>> {
        let adb = self.path().await;
        let mut cmd_args: Vec<String> = Vec::new();
        if let Some(s) = serial {
            cmd_args.push("-s".into());
            cmd_args.push(s.into());
        }
        cmd_args.extend(args.iter().map(|a| a.to_string()));
        let adb2 = adb.clone();
        let output = tauri::async_runtime::spawn_blocking(move || {
            let mut c = Command::new(&adb2);
            c.args(&cmd_args);
            crate::util::hide_console(&mut c);
            c.output()
        })
        .await
        .map_err(|e| AppError::Adb(e.to_string()))?
        .map_err(|e| AppError::Adb(format!("启动 adb 失败 ({adb:?}): {e}")))?;

        if !output.status.success() {
            return Err(AppError::Adb(format!(
                "adb 退出码 {}: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }

    /// 启动长驻子进程（投屏管道用）。
    pub async fn spawn(
        &self,
        serial: Option<&str>,
        args: &[&str],
        stdin: Stdio,
        stdout: Stdio,
        stderr: Stdio,
    ) -> AppResult<Child> {
        let adb = self.path().await;
        let mut cmd = Command::new(&adb);
        if let Some(s) = serial {
            cmd.arg("-s").arg(s);
        }
        cmd.args(args).stdin(stdin).stdout(stdout).stderr(stderr);
        crate::util::hide_console(&mut cmd);
        cmd.spawn().map_err(|e| AppError::Adb(e.to_string()))
    }

    /// 确保 adb server 已启动；版本不匹配时自动重启。
    pub async fn ensure_server(&self) -> AppResult<()> {
        let out = self.run(None, &["start-server"]).await?;
        if out.code != 0 && out.stderr.contains("server version") {
            let _ = self.run(None, &["kill-server"]).await;
            self.run(None, &["start-server"]).await?.ok()?;
        }
        Ok(())
    }
}

pub fn resolve_paths(cfg: &config::AppConfig) -> AppResult<(PathBuf, PathBuf)> {
    let adb = config::detect_adb(cfg.adb_path.as_deref())?;
    let ffmpeg = config::detect_ffmpeg(cfg.ffmpeg_path.as_deref())?;
    Ok((adb, ffmpeg))
}
