use super::AdbClient;
use crate::error::{AppError, AppResult};
use crate::util;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub product: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProps {
    pub serial: String,
    pub model: String,
    pub manufacturer: String,
    pub android_version: String,
    pub sdk: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub density: u32,
    pub battery_level: u32,
    pub awake: bool,
    pub rotation: u8,
}

impl AdbClient {
    pub async fn list_devices(&self) -> AppResult<Vec<DeviceInfo>> {
        self.ensure_server().await?;
        let out = self.run(None, &["devices", "-l"]).await?;
        let mut devices = Vec::new();
        for line in out.stdout.lines().skip(1) {
            if let Some((serial, state, model, product)) = util::parse_device_line(line) {
                devices.push(DeviceInfo {
                    serial,
                    state,
                    model,
                    product,
                });
            }
        }
        Ok(devices)
    }

    pub async fn props(&self, serial: &str) -> AppResult<DeviceProps> {
        let model = self.getprop(serial, "ro.product.model").await?;
        let manufacturer = self.getprop(serial, "ro.product.manufacturer").await?;
        let android_version = self.getprop(serial, "ro.build.version.release").await?;
        let sdk = self.getprop(serial, "ro.build.version.sdk").await?.parse().unwrap_or(0);

        let (screen_width, screen_height) = self.wm_size(serial).await?;
        let density = self.wm_density(serial).await?;
        let battery_level = self.battery_level(serial).await?;
        let awake = self
            .run(Some(serial), &["shell", "dumpsys", "power"])
            .await?
            .stdout
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("mWakefulness=")
                    .map(|v| v.trim().trim_matches(',') == "Awake")
            })
            .unwrap_or(false);
        let rotation = self
            .run(
                Some(serial),
                &["shell", "dumpsys", "input", "|", "grep", "-m1", "SurfaceOrientation"],
            )
            .await?
            .stdout
            .split_whitespace()
            .nth(1)
            .map(|v| v.trim().parse::<u8>().unwrap_or(0))
            .unwrap_or(0);

        Ok(DeviceProps {
            serial: serial.to_string(),
            model,
            manufacturer,
            android_version,
            sdk,
            screen_width,
            screen_height,
            density,
            battery_level,
            awake,
            rotation,
        })
    }

    pub async fn getprop(&self, serial: &str, key: &str) -> AppResult<String> {
        let out = self.run(Some(serial), &["shell", "getprop", key]).await?;
        Ok(out.stdout.trim().to_string())
    }

    pub async fn wm_size(&self, serial: &str) -> AppResult<(u32, u32)> {
        let out = self
            .run(Some(serial), &["shell", "wm", "size"])
            .await?
            .stdout;
        for line in out.lines() {
            if let Some(v) = line.strip_prefix("Physical size:") {
                let v = v.trim();
                if let Some((w, h)) = v.split_once('x') {
                    if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                        return Ok((w, h));
                    }
                }
            }
        }
        Err(AppError::Device("无法解析屏幕尺寸".into()))
    }

    pub async fn wm_density(&self, serial: &str) -> AppResult<u32> {
        let out = self.run(Some(serial), &["shell", "wm", "density"]).await?;
        for line in out.stdout.lines() {
            if let Some(v) = line.strip_prefix("Physical density:") {
                return Ok(v.trim().parse().unwrap_or(0));
            }
        }
        Ok(0)
    }

    pub async fn battery_level(&self, serial: &str) -> AppResult<u32> {
        let out = self.run(Some(serial), &["shell", "dumpsys", "battery"]).await?;
        Ok(out
            .stdout
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("level:")
                    .map(|v| v.trim().parse::<u32>().unwrap_or(0))
            })
            .unwrap_or(0))
    }
}
