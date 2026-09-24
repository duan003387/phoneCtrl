use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Mutex;
use std::time::Duration;

// ────────────────────────── Appium 生命周期（内置/全局/npx 三级） ──────────────────────────
//
// 解析顺序：
//   1. 内置便携运行时：把随包分发的 appium-bundle.tar.gz 解到 <config>/appium-runtime/
//      （含 bin/node 或 bin/node.exe、node_modules/appium、.appium 驱动），完全离线；
//   2. 系统全局 appium（PATH）；
//   3. npx --prefer-offline appium（首次可能联网，之后走缓存）。
// 都不可用时，运行用例报错并提示。

/// 首选端口；被非 Appium 进程占住时向后顺延（见 `acquire_port`）。
const APPIUM_PORT_FIRST: u16 = 4723;
const APPIUM_PORT_SPAN: u16 = 10;
/// npx 兜底与内置运行时必须同一主版本，否则同一套 WD 代码要同时适配两版 CLI
/// （版本以 scripts/bundle-appium.sh 的 APPIUM_VERSION 为准）。
const APPIUM_NPX_SPEC: &str = "appium@3";
/// 内置运行时目录布局版本，与 bundle 字节数一起写进 .ready：换包/升级即强制重解。
const APPIUM_BUNDLE_LAYOUT: &str = "3";

static APPIUM_CHILD: Mutex<Option<Child>> = Mutex::new(None);
/// 当前 Appium 服务端口，由 `ensure_appium` 选定；所有 WebDriver 请求据此拼 URL。
static APPIUM_PORT: AtomicU16 = AtomicU16::new(APPIUM_PORT_FIRST);

fn wd_base() -> String {
    format!("http://127.0.0.1:{}", APPIUM_PORT.load(Ordering::Relaxed))
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_default()
}

/// 健康/端口探测专用客户端：连到非 Appium 的监听者也不能挂住。
fn probe_http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_default()
}

/// 指定端口上的 GET /status 是否就绪。
async fn appium_ready_on(port: u16) -> bool {
    match probe_http()
        .get(format!("http://127.0.0.1:{port}/status"))
        .send()
        .await
    {
        Ok(r) => {
            let status = r.status();
            if !status.is_success() {
                return false;
            }
            match r.json::<Value>().await {
                Ok(v) => v
                    .get("value")
                    .and_then(|x| x.get("ready"))
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

/// 本机端口是否可绑定（即空闲）。Appium 未监听但端口被别的服务占住时，
/// 绑定失败能把它和「真正空闲」区分开。
fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// 选定服务端口：优先复用在跑的 Appium（免得重复起一份），否则取第一个空闲端口。
/// 返回 (端口, 是否已就绪可直接用)。全部被占时退回首选端口，让后续报错指向真实原因。
async fn acquire_port() -> (u16, bool) {
    let mut first_free = None;
    for port in APPIUM_PORT_FIRST..(APPIUM_PORT_FIRST + APPIUM_PORT_SPAN) {
        if appium_ready_on(port).await {
            return (port, true);
        }
        if first_free.is_none() && port_free(port) {
            first_free = Some(port);
        }
    }
    (first_free.unwrap_or(APPIUM_PORT_FIRST), false)
}

/// npm 家族在 Windows 上是 .cmd 批处理：CreateProcess 只自动补 .exe，不按 PATHEXT 解析，
/// 故必须显式带后缀才能 spawn（unix 下保持原名，行为不变）。
fn appium_programs() -> Vec<&'static str> {
    if cfg!(windows) {
        vec!["appium", "appium.cmd"]
    } else {
        vec!["appium"]
    }
}

fn npx_program() -> &'static str {
    if cfg!(windows) {
        "npx.cmd"
    } else {
        "npx"
    }
}

async fn run_cmd(program: &str, args: &[&str]) -> AppResult<(i32, String)> {
    let out = tokio::process::Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("启动 {program} 失败: {e}")))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok((out.status.code().unwrap_or(-1), combined))
}

/// 启动一种 appium 调用方式：程序 + 前缀参数 + 可选 APPIUM_HOME。
struct Launch {
    program: String,
    prefix: Vec<String>,
    apium_home: Option<std::path::PathBuf>,
    label: &'static str,
}

impl Launch {
    fn cmd(&self, extra: &[&str]) -> (String, Vec<String>) {
        let mut args: Vec<String> = self.prefix.iter().cloned().collect();
        args.extend(extra.iter().map(|s| s.to_string()));
        (self.program.clone(), args)
    }
}

/// 尝试从内置包解出便携运行时；返回其 appium 调用方式（若就绪）。
/// 注意：解压是重 IO 操作，调用方需放到阻塞线程里（见 `resolve_launch`）。
fn bundled_launch(config_dir: &std::path::Path) -> Option<Launch> {
    let bundle = crate::util::resolve_resource("appium-bundle.tar.gz")?;
    // 未跑 bundle-appium.sh 时，build.rs 会补一个 0 字节占位（只为让 tauri
    // bundle.resources 引用成立）。占位不是可用运行时，直接判不可用去走回退，
    // 否则解压必失败还会留下半截目录。
    let bundle_len = std::fs::metadata(&bundle).map(|m| m.len()).unwrap_or(0);
    if bundle_len == 0 {
        eprintln!("[automate] 内置 appium 包为占位空文件，跳过内置方式");
        return None;
    }
    let dest = config_dir.join("appium-runtime");
    let marker = dest.join(".ready");
    // .ready 存「布局版本|bundle 字节数」：只判断存在的话，升级 Appium 或换包后
    // 旧目录会被永久复用，改动永远不生效。
    let expect = format!("{APPIUM_BUNDLE_LAYOUT}|{bundle_len}");
    let fresh = std::fs::read_to_string(&marker)
        .map(|s| s.trim() == expect)
        .unwrap_or(false);
    if !fresh {
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::create_dir_all(&dest).ok()?;
        // 用系统 tar 解包（mac/linux 自带；Windows 10+ 亦有 tar）
        let ok = std::process::Command::new("tar")
            .args(["-xzf", &bundle.to_string_lossy(), "-C", &dest.to_string_lossy()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            // 半截目录留着会让下次误判为已就绪，清掉再回退
            let _ = std::fs::remove_dir_all(&dest);
            return None;
        }
        let _ = std::fs::write(&marker, expect.as_bytes());
    }
    // 落盘名由打包机的 OS 决定（scripts/bundle-appium.sh 按 uname 选择 node / node.exe）
    let node = dest.join("bin").join(if cfg!(windows) { "node.exe" } else { "node" });
    if !node.exists() {
        eprintln!("[automate] 内置运行时缺少 {}，回退其它方式", node.display());
        return None;
    }
    // appium CLI 入口（与 scripts/bundle-appium.sh 保持同一优先级：index.js 优先）
    let cli = ["node_modules/appium/index.js", "node_modules/appium/build/lib/main.js"]
        .iter()
        .map(|p| dest.join(p))
        .find(|p| p.exists())?;
    Some(Launch {
        program: node.to_string_lossy().into_owned(),
        prefix: vec![cli.to_string_lossy().into_owned()],
        apium_home: Some(dest.join(".appium")),
        label: "内置",
    })
}

/// 选择可用的 appium 调用方式。
async fn resolve_launch(config_dir: &std::path::Path) -> Launch {
    // 内置包 >100M，解压可达数十秒：放阻塞线程，避免卡住 IPC 与投屏等其它命令
    let dir_owned = config_dir.to_path_buf();
    if let Ok(Some(l)) =
        tauri::async_runtime::spawn_blocking(move || bundled_launch(&dir_owned)).await
    {
        return l;
    }
    for cand in appium_programs() {
        if run_cmd(cand, &["--version"])
            .await
            .map(|(c, _)| c == 0)
            .unwrap_or(false)
        {
            return Launch {
                program: cand.into(),
                prefix: vec![],
                apium_home: None,
                label: "全局",
            };
        }
    }
    Launch {
        program: npx_program().into(),
        prefix: vec!["--yes".into(), "--prefer-offline".into(), APPIUM_NPX_SPEC.into()],
        apium_home: None,
        label: "npx",
    }
}

/// 确保 appium server 就绪并按需拉起。
pub async fn ensure_appium(config_dir: &std::path::Path) -> AppResult<()> {
    let (port, reused) = acquire_port().await;
    APPIUM_PORT.store(port, Ordering::Relaxed);
    if reused {
        return Ok(());
    }
    let launch = resolve_launch(config_dir).await;
    eprintln!("[automate] appium 启动方式：{}，端口 {port}", launch.label);

    // 内置方式驱动已随包预装，无需安装；全局/npx 需确保 uiautomator2 驱动存在。
    if launch.label != "内置" {
        if launch.label == "npx" {
            // 触发 npx 拉取 appium（首次联网，之后 --prefer-offline 走缓存）
            let (code, log) = run_cmd(
                &launch.program,
                &["--yes", "--prefer-offline", APPIUM_NPX_SPEC, "--version"],
            )
            .await?;
            if code != 0 {
                return Err(AppError::Config(format!("安装/检测 Appium 失败：{log}")));
            }
        }
        // 已安装则跳过，避免 "already installed" 的非零退出误报
        let (lc, list) = run_cmd_for(&launch, &["driver", "list", "--installed"]).await;
        let installed = lc == 0 && list.contains("uiautomator2");
        if !installed {
            let (c, log) = run_cmd_for(&launch, &["driver", "install", "uiautomator2"]).await;
            if c != 0
                && !log.contains("already exists")
                && !log.contains("already installed")
            {
                eprintln!("[automate] uiautomator2 驱动安装失败：{log}");
            }
        }
    }

    // 后台拉起服务
    {
        let mut guard = APPIUM_CHILD.lock().unwrap();
        let alive = guard
            .as_mut()
            .map(|c| c.try_wait().map(|s| s.is_none()).unwrap_or(false))
            == Some(true);
        if !alive {
            // 上一轮的子进程若已退出，先收掉：既避免僵尸，也让它占的资源（含端口）释放。
            // std::process::Child 不会在 drop 时杀进程，不显式 reap 就会残留。
            if let Some(mut dead) = guard.take() {
                let _ = dead.kill();
                let _ = dead.wait();
            }
            let (program, args) = launch.cmd(&[
                "--port",
                &port.to_string(),
                "--log-level",
                "error",
            ]);
            let mut cb = std::process::Command::new(&program);
            cb.args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Some(home) = &launch.apium_home {
                cb.env("APPIUM_HOME", home);
            }
            #[cfg(windows)]
            {
                // CREATE_NO_WINDOW：服务是后台常驻的，别在桌面上弹一扇 node 控制台
                use std::os::windows::process::CommandExt;
                cb.creation_flags(0x0800_0000);
            }
            let child = cb
                .spawn()
                .map_err(|e| AppError::Config(format!("启动 appium server 失败({}): {e}", launch.label)))?;
            *guard = Some(child);
        }
    }
    // 轮询健康检查（最多 ~60s，内置更快）
    let tries = if launch.label == "内置" { 60 } else { 120 };
    for _ in 0..tries {
        if appium_ready_on(port).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    // 失败原因是 Appium 运行时/端口，跟设备连接无关（会话还没建呢），文案别再指向设备。
    Err(AppError::Config(format!(
        "Appium 服务在端口 {port} 上未在 {}s 内就绪（启动方式：{}）。请确认内置运行时完整，或自行启动 Appium 后重试。",
        tries / 2,
        launch.label,
    )))
}

// 小工具：以 Launch 的方式执行一条命令（带 APPIUM_HOME 环境）
async fn run_cmd_for(l: &Launch, extra: &[&str]) -> (i32, String) {
    let (program, args) = l.cmd(extra);
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let owned = program;
    if let Some(home) = &l.apium_home {
        let out = tokio::process::Command::new(&owned)
            .args(&refs)
            .env("APPIUM_HOME", home)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        match out {
            Ok(o) => (
                o.status.code().unwrap_or(-1),
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                ),
            ),
            Err(e) => (-1, e.to_string()),
        }
    } else {
        match run_cmd(&owned, &refs).await {
            Ok(v) => v,
            Err(e) => (-1, e.to_string()),
        }
    }
}

// ────────────────────────── WebDriver 会话 ──────────────────────────

struct Session {
    id: String,
}

impl Session {
    async fn create(serial: &str, adb: Option<&std::path::Path>) -> AppResult<Session> {
        // 显式告知 Appium 用哪个 adb：优先我们内置/探测到的 adb，
        // 否则目标机没装 Android SDK 时 Appium 找不到 adb 会失败。
        let mut caps = json!({
            "platformName": "Android",
            "appium:automationName": "UiAutomator2",
            "appium:udid": serial,
            "appium:newCommandTimeout": 300,
            "appium:adbExecTimeout": 60000,
            "appium:skipServerInstallation": false,
            "appium:autoGrantPermissions": true
        });
        if let Some(a) = adb {
            caps["appium:adbExecutable"] = json!(a.to_string_lossy());
        }
        let body = json!({
            "capabilities": {
                "alwaysMatch": caps.clone(),
                "firstMatch": [{}]
            },
            "desiredCapabilities": caps
        });
        let resp = http()
            .post(format!("{}/session", wd_base()))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Config(format!("创建 Appium 会话失败: {e}")))?;
        let status = resp.status();
        let v: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Config(format!("会话响应解析失败: {e}")))?;
        if !status.is_success() {
            return Err(AppError::Config(format!(
                "Appium 会话创建失败: {}",
                v.get("value")
                    .and_then(|x| x.get("message"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("未知错误")
            )));
        }
        let id = v
            .get("value")
            .and_then(|x| x.get("sessionId"))
            .and_then(|x| x.as_str())
            .or_else(|| v.get("sessionId").and_then(|x| x.as_str()))
            .ok_or_else(|| AppError::Config("会话响应缺少 sessionId".into()))?
            .to_string();
        Ok(Session { id })
    }

    async fn delete(&self) {
        let _ = http()
            .delete(format!("{}/session/{}", wd_base(), self.id))
            .send()
            .await;
    }

    fn url(&self, path: &str) -> String {
        format!("{}/session/{}{}", wd_base(), self.id, path)
    }

    async fn post(&self, path: &str, body: &Value) -> AppResult<Value> {
        let r = http()
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .map_err(|e| AppError::Config(format!(" WebDriver 请求失败: {e}")))?;
        let v: Value = r
            .json()
            .await
            .map_err(|e| AppError::Config(format!("WebDriver 响应解析失败: {e}")))?;
        Ok(v)
    }

    async fn get(&self, path: &str) -> AppResult<Value> {
        let r = http()
            .get(self.url(path))
            .send()
            .await
            .map_err(|e| AppError::Config(format!("WebDriver 请求失败: {e}")))?;
        let v: Value = r
            .json()
            .await
            .map_err(|e| AppError::Config(format!("WebDriver 响应解析失败: {e}")))?;
        Ok(v)
    }

    async fn execute(&self, script: &str, args: Value) -> AppResult<Value> {
        self.post("/execute/sync", &json!({ "script": script, "args": [args] })).await
    }

    async fn find(&self, using: &str, selector: &str) -> Option<String> {
        let v = self
            .post("/element", &json!({ "using": using, "value": selector }))
            .await
            .ok()?;
        element_id(v.get("value")?)
    }

    async fn source(&self) -> Option<String> {
        let v = self.get("/source").await.ok()?;
        v.get("value")?.as_str().map(|s| s.to_string())
    }

    async fn screenshot(&self) -> Option<String> {
        let v = self.get("/screenshot").await.ok()?;
        v.get("value")?.as_str().map(|s| s.to_string())
    }

    async fn window_size(&self) -> (u32, u32) {
        if let Ok(v) = self.get("/window/current/rect").await {
            if let Some(wh) = parse_size(&v) {
                return wh;
            }
        }
        if let Ok(v) = self.execute("mobile: getDeviceSize", json!({})).await {
            let w = v.get("value").and_then(|x| x.get("width")).and_then(|x| x.as_u64()).unwrap_or(1080) as u32;
            let h = v.get("value").and_then(|x| x.get("height")).and_then(|x| x.as_u64()).unwrap_or(1920) as u32;
            return (w, h);
        }
        (1080, 1920)
    }
}

fn element_id(v: &Value) -> Option<String> {
    const W3C: &str = "element-6066-11e4-a52e-4f735466cecf";
    if let Some(s) = v.get(W3C).and_then(|x| x.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = v.get("ELEMENT").and_then(|x| x.as_str()) {
        return Some(s.to_string());
    }
    None
}

fn parse_size(v: &Value) -> Option<(u32, u32)> {
    let val = v.get("value")?;
    let w = val.get("width")?.as_u64()? as u32;
    let h = val.get("height")?.as_u64()? as u32;
    Some((w, h))
}

/// 定位方式 → (using, selector)。
fn locator(by: &str, value: &str) -> (String, String) {
    match by {
        "id" => ("id".into(), value.into()),
        "accessibilityId" => ("accessibility id".into(), value.into()),
        "xpath" => ("xpath".into(), value.into()),
        "className" => ("class name".into(), value.into()),
        // text：用 UiScrollable/UiSelector 匹配精确文本
        _ => (
            "-android uiautomator".into(),
            format!("new UiSelector().text(\"{}\")", value.replace('"', "\\\"")),
        ),
    }
}

// ────────────────────────── 用例步骤 DSL ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Step {
    #[serde(rename_all = "camelCase")]
    OpenApp { package: String },
    #[serde(rename_all = "camelCase")]
    Tap {
        #[serde(default)]
        by: Option<String>,
        #[serde(default)]
        value: Option<String>,
        #[serde(default)]
        x: Option<u32>,
        #[serde(default)]
        y: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    Input {
        #[serde(default)]
        by: Option<String>,
        #[serde(default)]
        value: Option<String>,
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Swipe { direction: String },
    #[serde(rename_all = "camelCase")]
    Wait { ms: u64 },
    #[serde(rename_all = "camelCase")]
    WaitFor {
        by: String,
        value: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    AssertText {
        contains: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Key { keycode: u16 },
    Screenshot,
    Back,
    Home,
}

impl Step {
    fn label(&self) -> String {
        match self {
            Step::OpenApp { package } => format!("启动应用 {package}"),
            Step::Tap { by, value, x, y } => match (by, value) {
                (Some(b), Some(v)) => format!("点击 [{b}={v}]"),
                _ => format!("点击坐标 ({x:?},{y:?})"),
            },
            Step::Input { text, .. } => format!("输入 “{text}”"),
            Step::Swipe { direction } => format!("滑动 {direction}"),
            Step::Wait { ms } => format!("等待 {ms}ms"),
            Step::WaitFor { by, value, .. } => format!("等待出现 [{by}={value}]"),
            Step::AssertText { contains, .. } => format!("断言含文本 “{contains}”"),
            Step::Key { keycode } => format!("按键 {keycode}"),
            Step::Screenshot => "截图".into(),
            Step::Back => "返回".into(),
            Step::Home => "主屏".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub index: usize,
    pub label: String,
    pub ok: bool,
    pub message: String,
    pub elapsed_ms: u64,
    pub screenshot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub ok: bool,
    pub steps: Vec<StepResult>,
    pub elapsed_ms: u64,
}

// ────────────────────────── 执行器 ──────────────────────────

pub async fn run_steps(config_dir: &std::path::Path, serial: &str, steps: Vec<Step>) -> AppResult<RunReport> {
    ensure_appium(config_dir).await?;
    // 让 Appium 复用我们探测/内置的 adb（干净机器无 Android SDK 也能驱动设备）。
    let adb = crate::config::detect_adb(None).ok();
    let session = Session::create(serial, adb.as_deref()).await?;
    let mut results: Vec<StepResult> = Vec::with_capacity(steps.len());
    let started = std::time::Instant::now();
    let mut all_ok = true;

    for (i, step) in steps.iter().enumerate() {
        let t0 = std::time::Instant::now();
        let outcome = exec_step(&session, step).await;
        let elapsed = t0.elapsed().as_millis() as u64;
        let (ok, message) = match outcome {
            Ok(msg) => (true, msg),
            Err(e) => (false, e.to_string()),
        };
        if !ok {
            all_ok = false;
        }
        // 失败时抓一张当前画面便于定位（截图本身失败也不影响）
        let shot = if ok { None } else { session.screenshot().await };
        results.push(StepResult {
            index: i,
            label: step.label(),
            ok,
            message,
            elapsed_ms: elapsed,
            screenshot: shot,
        });
        if !ok {
            break; // 首个失败即中止（失败即停，报表清晰）
        }
    }

    session.delete().await;
    Ok(RunReport {
        ok: all_ok,
        steps: results,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

async fn exec_step(s: &Session, step: &Step) -> AppResult<String> {
    match step {
        Step::OpenApp { package } => {
            let v = s.execute("mobile: activateApp", json!({ "appId": package })).await?;
            wd_err(&v)?;
            Ok(format!("已激活 {package}"))
        }
        Step::Tap { by, value, x, y } => {
            if let (Some(b), Some(v)) = (by.as_deref(), value.as_deref()) {
                let (using, sel) = locator(b, v);
                let eid = s.find(&using, &sel).await.ok_or_else(|| {
                    AppError::Config(format!("未找到控件 [{using}={sel}]"))
                })?;
                let r = s
                    .post(&format!("/element/{eid}/click"), &json!({}))
                    .await?;
                wd_err(&r)?;
                Ok(format!("已点击 [{b}={v}]"))
            } else if let (Some(px), Some(py)) = (*x, *y) {
                let r = s.execute("mobile: clickGesture", json!({ "x": px, "y": py })).await?;
                wd_err(&r)?;
                Ok(format!("已点击坐标 ({px},{py})"))
            } else {
                Err(AppError::Config("点击步骤缺少定位或坐标".into()))
            }
        }
        Step::Input { by, value, text } => {
            let (b, v) = (by.as_deref().unwrap_or("text"), value.clone().unwrap_or_default());
            let (using, sel) = locator(b, &v);
            let eid = s.find(&using, &sel).await.ok_or_else(|| {
                AppError::Config(format!("未找到输入框 [{using}={sel}]"))
            })?;
            let r = s
                .post(&format!("/element/{eid}/value"), &json!({ "text": text }))
                .await?;
            wd_err(&r)?;
            Ok(format!("已输入 “{text}”"))
        }
        Step::Swipe { direction } => {
            let (w, h) = s.window_size().await;
            let r = s
                .execute(
                    "mobile: swipeGesture",
                    json!({
                        "left": 0, "top": 0, "width": w, "height": h,
                        "direction": direction_cap(direction),
                        "percent": 0.75, "speed": 3000
                    }),
                )
                .await?;
            wd_err(&r)?;
            Ok(format!("已向 {direction} 滑动"))
        }
        Step::Wait { ms } => {
            tokio::time::sleep(Duration::from_millis(*ms)).await;
            Ok(format!("等待 {ms}ms"))
        }
        Step::WaitFor { by, value, timeout_ms } => {
            let (using, sel) = locator(by, value);
            let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms.unwrap_or(8000));
            loop {
                if s.find(&using, &sel).await.is_some() {
                    return Ok(format!("已出现 [{by}={value}]"));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(AppError::Config(format!("等待超时 [{by}={value}]")));
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        }
        Step::AssertText { contains, timeout_ms } => {
            let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms.unwrap_or(2000));
            loop {
                let src = s.source().await.unwrap_or_default();
                if src.contains(contains.as_str()) {
                    return Ok(format!("页面含文本 “{contains}”"));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(AppError::Config(format!("页面未找到文本 “{contains}”")));
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
        Step::Key { keycode } => {
            let r = s.execute("mobile: pressKey", json!({ "keycode": keycode })).await?;
            wd_err(&r)?;
            Ok(format!("已发送按键 {keycode}"))
        }
        Step::Screenshot => Ok("已截图".into()),
        Step::Back => {
            let r = s.execute("mobile: pressKey", json!({ "keycode": 4 })).await?;
            wd_err(&r)?;
            Ok("已返回".into())
        }
        Step::Home => {
            let r = s.execute("mobile: pressKey", json!({ "keycode": 3 })).await?;
            wd_err(&r)?;
            Ok("已回主屏".into())
        }
    }
}

fn direction_cap(d: &str) -> &str {
    match d {
        "up" | "Up" => "up",
        "down" | "Down" => "down",
        "left" | "Left" => "left",
        _ => "right",
    }
}

/// Appium/W3C 响应里 value.error 或 status!=0 视为错误。
fn wd_err(v: &Value) -> AppResult<()> {
    if let Some(err) = v.get("value").and_then(|x| x.get("error")).and_then(|x| x.as_str()) {
        let msg = v
            .get("value")
            .and_then(|x| x.get("message"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        return Err(AppError::Config(format!("{err}: {msg}")));
    }
    Ok(())
}

/// 供自动化页探测环境：将采用哪种 appium 方式、是否已就绪。
pub async fn env_status(config_dir: &std::path::Path) -> Value {
    let launch = resolve_launch(config_dir).await;
    json!({
        "mode": launch.label,
        "appiumReady": appium_ready_on(APPIUM_PORT.load(Ordering::Relaxed)).await,
    })
}

// ────────────────────────── Tauri 命令 ──────────────────────────

use tauri::command;
use tauri::State;
use crate::state::AppState;

#[command]
pub async fn automate_run(
    state: State<'_, AppState>,
    serial: String,
    steps: Vec<Step>,
) -> AppResult<RunReport> {
    let config_dir = state.config_dir.clone();
    run_steps(&config_dir, &serial, steps).await
}

#[command]
pub async fn automate_env(state: State<'_, AppState>) -> AppResult<Value> {
    let config_dir = state.config_dir.clone();
    Ok(env_status(&config_dir).await)
}
