use super::AdbClient;
use crate::error::{AppError, AppResult};
use crate::util;
use base64::Engine;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub permissions: String,
}

impl AdbClient {
    pub async fn list_files(&self, serial: &str, path: &str) -> AppResult<Vec<FileEntry>> {
        // 尾随斜杠可解析符号链接目录（如 /sdcard → /storage/emulated/0）
        let ls_path = format!("{}/", path.trim_end_matches('/'));
        let quoted = util::shell_quote(&ls_path);
        let out = self
            .run(Some(serial), &["shell", "ls", "-la", &quoted])
            .await?;
        if out.code != 0 {
            return Err(AppError::Adb(format!(
                "无法列出目录 {}: {}",
                path,
                out.stderr.trim()
            )));
        }
        let mut entries = Vec::new();
        for line in out.stdout.lines() {
            if let Some((name, size, is_dir, perms)) = util::parse_ls_line(line) {
                if name == "." || name == ".." {
                    continue;
                }
                entries.push(FileEntry {
                    path: format!("{}/{}", path.trim_end_matches('/'), name),
                    name,
                    is_dir,
                    size,
                    permissions: perms,
                });
            }
        }
        Ok(entries)
    }

    pub async fn delete_path(&self, serial: &str, path: &str) -> AppResult<()> {
        let quoted = util::shell_quote(path);
        self.run(Some(serial), &["shell", "rm", "-rf", &quoted]).await?.ok()
    }

    pub async fn mkdir_path(&self, serial: &str, path: &str) -> AppResult<()> {
        let quoted = util::shell_quote(path);
        self.run(Some(serial), &["shell", "mkdir", "-p", &quoted]).await?.ok()
    }

    pub async fn rename_path(&self, serial: &str, from: &str, to: &str) -> AppResult<()> {
        let qf = util::shell_quote(from);
        let qt = util::shell_quote(to);
        self.run(Some(serial), &["shell", "mv", &qf, &qt]).await?.ok()
    }

    pub async fn copy_path(&self, serial: &str, from: &str, to: &str) -> AppResult<()> {
        let qf = util::shell_quote(from);
        let qt = util::shell_quote(to);
        self.run(Some(serial), &["shell", "cp", "-r", &qf, &qt]).await?.ok()
    }

    /// 上传文件到设备（adb push）。
    pub async fn upload(&self, serial: &str, local: &str, remote: &str) -> AppResult<()> {
        self.ensure_server().await?;
        let out = self
            .run(Some(serial), &["push", local, remote])
            .await?;
        if out.code != 0 {
            return Err(AppError::Adb(format!(
                "上传失败 {} -> {}: {}",
                local,
                remote,
                out.stderr.trim()
            )));
        }
        Ok(())
    }

    /// 以字节数组上传：前端读到文件内容后经 IPC 传入，落临时文件再 push。
    pub async fn upload_bytes(
        &self,
        serial: &str,
        remote_dir: &str,
        name: &str,
        data: Vec<u8>,
    ) -> AppResult<()> {
        let tmp = std::env::temp_dir().join(format!(
            "phonectrl_up_{}_{}",
            uuid::Uuid::new_v4().simple(),
            name
        ));
        std::fs::write(&tmp, data)?;
        let remote = format!("{}/{}", remote_dir.trim_end_matches('/'), name);
        let res = self.upload(serial, &tmp.to_string_lossy(), &remote).await;
        let _ = std::fs::remove_file(&tmp);
        res
    }

    /// 从设备下载文件到本地（adb pull）。
    pub async fn download(&self, serial: &str, remote: &str, local: &str) -> AppResult<()> {
        self.ensure_server().await?;
        let out = self
            .run(Some(serial), &["pull", remote, local])
            .await?;
        if out.code != 0 {
            return Err(AppError::Adb(format!(
                "下载失败 {} -> {}: {}",
                remote,
                local,
                out.stderr.trim()
            )));
        }
        Ok(())
    }

    /// 读取小文件内容预览（base64 返回，供前端展示）。
    pub async fn read_preview(&self, serial: &str, path: &str, max: usize) -> AppResult<String> {
        let mut data = self.run_raw(Some(serial), &["exec-out", "cat", path]).await?;
        if data.len() > max {
            data.truncate(max);
        }
        Ok(base64::engine::general_purpose::STANDARD.encode(&data))
    }
}
