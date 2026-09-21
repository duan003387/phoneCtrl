use crate::error::{AppError, AppResult};
use tokio::io::AsyncWriteExt;

// scrcpy 1.25 控制协议常量
const TYPE_INJECT_KEYCODE: u8 = 0;
const TYPE_INJECT_TEXT: u8 = 1;
const TYPE_INJECT_TOUCH_EVENT: u8 = 2;
const TYPE_SET_CLIPBOARD: u8 = 9;

pub const ACTION_DOWN: u8 = 0;
pub const ACTION_UP: u8 = 1;
pub const ACTION_MOVE: u8 = 2;

const POINTER_ID_FINGER: u64 = u64::MAX; // -1，通用手指
const PRESSURE_ON: f32 = 1.0;
const PRESSURE_OFF: f32 = 0.0;

/// scrcpy 服务器控制通道：直接向设备注入输入，延迟远低于 adb shell input。
pub struct ControlClient {
    stream: tokio::net::TcpStream,
}

impl ControlClient {
    pub async fn connect(port: u16) -> AppResult<Self> {
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|e| AppError::Stream(format!("连接控制通道失败: {e}")))?;
        Ok(Self { stream })
    }

    fn to_u16fp(v: f32) -> u16 {
        ((v * 65536.0) as u64).min(0xFFFF) as u16
    }

    /// 注入触摸事件。x/y 为设备逻辑坐标，w/h 为当前逻辑屏幕尺寸（用于归一化）。
    pub async fn touch(
        &mut self,
        action: u8,
        x: u32,
        y: u32,
        w: u16,
        h: u16,
    ) -> AppResult<()> {
        // scrcpy 3.x 触摸消息: type(1) action(1) pointerId(8) x(4) y(4) sw(2) sh(2)
        //                       pressure(2) actionButton(4) buttons(4) = 32 字节
        let mut buf = [0u8; 32];
        buf[0] = TYPE_INJECT_TOUCH_EVENT;
        buf[1] = action;
        buf[2..10].copy_from_slice(&POINTER_ID_FINGER.to_be_bytes());
        // x/y 为 16.16 定点数
        buf[10..14].copy_from_slice(&((x as u64 * 0x10000) as u32).to_be_bytes());
        buf[14..18].copy_from_slice(&((y as u64 * 0x10000) as u32).to_be_bytes());
        buf[18..20].copy_from_slice(&w.to_be_bytes());
        buf[20..22].copy_from_slice(&h.to_be_bytes());
        let pressure = if action == ACTION_UP {
            PRESSURE_OFF
        } else {
            PRESSURE_ON
        };
        buf[22..24].copy_from_slice(&Self::to_u16fp(pressure).to_be_bytes());
        buf[24..28].copy_from_slice(&0u32.to_be_bytes()); // actionButton
        buf[28..32].copy_from_slice(&0u32.to_be_bytes()); // buttons
        self.write_all(&buf).await
    }

    /// 注入按键（down + up）。
    pub async fn keycode(&mut self, keycode: u32) -> AppResult<()> {
        for action in [0u8, 1u8] {
            let mut buf = [0u8; 14];
            buf[0] = TYPE_INJECT_KEYCODE;
            buf[1] = action;
            buf[2..6].copy_from_slice(&keycode.to_be_bytes());
            buf[6..10].copy_from_slice(&0u32.to_be_bytes()); // repeat
            buf[10..14].copy_from_slice(&0u32.to_be_bytes()); // metaState
            self.write_all(&buf).await?;
        }
        Ok(())
    }

    /// 注入文本（ASCII，scrcpy 服务器按字符键入）。
    pub async fn text(&mut self, text: &str) -> AppResult<()> {
        let bytes = text.as_bytes();
        if bytes.len() > 300 {
            return Err(AppError::Stream("文本过长".into()));
        }
        let mut buf = Vec::with_capacity(5 + bytes.len());
        buf.push(TYPE_INJECT_TEXT);
        buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(bytes);
        self.write_all(&buf).await
    }

    /// 非 ASCII 文本：写剪贴板并触发粘贴（scrcpy 服务器原生实现，全设备可用）。
    pub async fn clipboard_and_paste(&mut self, text: &str) -> AppResult<()> {
        let bytes = text.as_bytes();
        let mut buf = Vec::with_capacity(14 + bytes.len());
        buf.push(TYPE_SET_CLIPBOARD);
        buf.extend_from_slice(&1u64.to_be_bytes()); // sequence
        buf.push(1); // paste
        buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(bytes);
        self.write_all(&buf).await
    }

    async fn write_all(&mut self, buf: &[u8]) -> AppResult<()> {
        self.stream
            .write_all(buf)
            .await
            .map_err(|e| AppError::Stream(format!("控制通道写入失败: {e}")))
    }
}

/// 可被流生命周期更新的控制通道槽位。
#[derive(Clone)]
pub struct ControlSlot(pub std::sync::Arc<tokio::sync::Mutex<Option<ControlClient>>>);

impl Default for ControlSlot {
    fn default() -> Self {
        Self(std::sync::Arc::new(tokio::sync::Mutex::new(None)))
    }
}
