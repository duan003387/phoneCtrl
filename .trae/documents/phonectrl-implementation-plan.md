# PhoneCtrl — 跨平台安卓设备控制桌面 App 实施计划

## Context（背景）

用户需要一个 Windows + macOS 双平台桌面 App，通过 USB 连接并控制安卓设备。当前目录 `/Users/ltsy/Desktop/phoneCtrl` 为空，从零开始。

**已确认需求**：
- 技术栈：Tauri v2（Rust 后端）+ React/TS 前端
- 功能：① 投屏 + 鼠标键盘控制 ② 文件管理 ③ 应用管理 ④ 快捷指令/自动化（宏）
- 连接：USB（ADB）

**环境**（已核实）：macOS arm64，Rust 1.99 nightly、Node 22、Xcode 26.6；adb v36 位于 `~/Library/Android/sdk/platform-tools/adb`，已连接 2 台设备（HUAWEI `MDX0220422024078`、SAMSUNG `R5GYC2PZDGW`）；ffmpeg 8.1.2 位于 `/opt/homebrew/bin/ffmpeg`。adb/ffmpeg 为运行时依赖，App 自动探测路径 + 设置页可手动配置。

**核心架构决策**：
- ADB 交互：Rust 侧 `std::process` 调用 adb 二进制（不引入纯 Rust adb crate、不装 shell 插件）
- 投屏管道：`adb exec-out screenrecord --output-format=h264 ... -` → ffmpeg 解码转 MJPEG → **Tauri v2 IPC Channel（`Channel<InvokeResponseBody::Raw>`）推原始 JPEG 帧** → 前端 `img`/canvas 渲染（省掉本地 WebSocket 服务；若 Channel 二进制实测有问题再回退 tokio-tungstenite）
- 配置持久化：Rust `std::fs` 写 `app_config_dir/config.json`，无需 store 插件

## 目录结构

```
phoneCtrl/
├── package.json  vite.config.ts  tsconfig.json  index.html
├── src/                        # React 前端
│   ├── main.tsx  App.tsx  types.ts
│   ├── api/                    # 类型化 invoke 封装：devices.ts stream.ts input.ts files.ts apps.ts actions.ts macros.ts
│   ├── hooks/                  # useDeviceList.ts useStream.ts useMacroRecorder.ts
│   ├── components/             # DeviceBar MirrorCanvas ControlOverlay FilePanel AppPanel MacroPanel SettingsDialog StatusBar
│   └── styles/
├── .github/workflows/release.yml   # Windows/macOS 分平台构建
└── src-tauri/
    ├── Cargo.toml  tauri.conf.json  capabilities/default.json  build.rs  icons/
    └── src/
        ├── main.rs  lib.rs            # lib.rs: Builder + AppState + invoke_handler + 事件
        ├── error.rs  config.rs  state.rs  util.rs  ffmpeg.rs
        ├── adb/{mod,devices,input,files,apps,media}.rs
        ├── stream/{mod,pipeline,framing}.rs
        ├── macros.rs  commands.rs
```

## Rust 后端设计

### 状态与配置
- `AppState { config: RwLock<AppConfig>, adb: AdbClient, ffmpeg: FfmpegBin, streams: Mutex<HashMap<serial, StreamHandle>>, input: InputQueue, macros: MacroStore }`
- `AppConfig { adb_path?, ffmpeg_path?, stream_bitrate(默认4M), stream_max_width(默认720), stream_fps(默认20), stream_quality(默认4) }`
- adb 路径探测：设置值 > `ANDROID_HOME/platform-tools` > `ANDROID_SDK_ROOT` > `which adb` > macOS `~/Library/Android/sdk/platform-tools` > Windows `%LOCALAPPDATA%\Android\Sdk\platform-tools`
- ffmpeg 探测：设置值 > `which ffmpeg` > `/opt/homebrew/bin/ffmpeg` > Windows `where ffmpeg`
- `InputQueue`：mpsc 串行 worker 执行所有 input 命令，避免宏/连点打爆进程

### AdbClient（adb/mod.rs）
- `run(serial, args)` / `run_raw(serial, args)`（exec-out 二进制）/ `spawn(serial, args, stdin, stdout, stderr)`
- 异步经 `tauri::async_runtime::spawn_blocking`；首启 `start-server`，版本不匹配时 kill-server 重启
- `shell_quote`：mksh 单引号转义；`input text` 空格转 `%s`

### 核心函数（按域）
- **devices**：`list() -> Vec<DeviceInfo>`；`props(serial) -> DeviceProps`（getprop + `wm size/density` + dumpsys battery/power/input 方向）
- **input**（串行）：`tap / swipe / keyevent / text(ASCII) / text_unicode(剪贴板+PASTE 键，Android 10+) / motionevent`
- **files**：`list`（`ls -la` 解析）、`delete/mkdir/rename/copy`、`upload(push)/download(pull)`、`read_preview`（exec-out cat 转 base64）
- **apps**：`list(pm list packages)`、`install(-r)/uninstall/launch(resolve-activity+am start)/stop(force-stop)/detail(dumpsys)`；v1 显示包名（label 获取成本高）
- **media**（快捷指令）：`screenshot`（screencap→base64）、`screenshot_save`（存 Downloads）、`record`（设备端 `screenrecord --time-limit N` 成片→pull→rm，避免 stdout 中断导致 moov 未收尾）
- **macros**：`MacroStep {Tap/Swipe/Key/Text/Wait/Screenshot/Home/Back/Recents}` + `(相对ms, step)` 时间戳序列；`play(serial, macro, onProgress: Channel<u32>)` 按相对间隔回放，动作走 InputQueue，delta clamp(0,3000)

### Tauri command 清单
`devices_list` `device_start_server` `device_props` `settings_get` `settings_set` `diagnostics` | `stream_start(serial, opts, onFrame: Channel)` `stream_stop` `stream_status` | `input_tap/swipe/key/text/motionevent` | `files_list/delete/mkdir/rename/copy/upload/download/read` | `apps_list/install/uninstall/launch/stop/info` | `action_screenshot/screenshot_save/record/power/home/back/recents/volume_up/volume_down/wake` | `macro_save/list/delete/play`

Rust 向上发事件：`device://list-changed`、`stream://state`、`macro://progress`、`adb://lost`

## 投屏管道（stream/pipeline.rs）

```
loop {
  spawn adb exec-out screenrecord --output-format=h264 --size WxH(偶数!) --bit-rate B --time-limit 170 -
  spawn ffmpeg -f h264 -i pipe:0 -an -c:v mjpeg -q:v 4 -r 20 -f mjpeg pipe:1  (stdin 接 adb stdout)
  读帧线程: 扫描 FFD8..FFD9 分帧 → watch::Sender<Arc<Vec<u8>>> 最新帧槽（coalescing，防积压）
  转发任务: watch.changed() → channel.send(InvokeResponseBody::Raw(frame))
  select! {
    170s 到   => kill 两进程, continue          // 避开 screenrecord 180s 硬限
    ffmpeg 退出 => 指数退避(0.5→2s)重启, continue  // 设备端意外退出/拔线
    停止信号   => kill, break
  }
}
停止后 best-effort `adb shell pkill -f screenrecord` 清理僵尸流
```

- 终止用 `Child::kill()`（SIGKILL）即可，无需 SIGINT（实时直播不关心尾帧）
- 流分辨率：设备逻辑分辨率等比缩至 max_width 后**向上取偶**（H.264 约束）

### 坐标映射（前端）
设备原生分辨率 × 流分辨率 × canvas 显示矩形（object-fit: contain 去黑边）三层等比换算；手势聚合：位移<15px 且 <300ms → tap，否则 → swipe。旋转变化时 5s 轮询 `SurfaceOrientation` 并自动重启流。

## Tauri 配置要点
- `tauri.conf.json`：identifier `com.phonectrl.desktop`（勿以 .app 结尾）；devUrl 端口 1420 与 vite 一致；`csp: null`（调试期，发布前收紧）
- `capabilities/default.json`：只需 `core:default`（覆盖 event/invoke/window/path，含 Channel）；不装 shell 插件
- 插件：建议 `tauri-plugin-log`（排查 adb 输出）；不装 store/dialog/fs

## 依赖版本（2026-09 核实）
- tauri crate `2.11`、tauri-build `2.6`、`@tauri-apps/cli ^2.11.4`、`@tauri-apps/api ^2.11.0`
- tokio 1（rt-multi-thread/macros/process/io-util/sync/time）、serde/serde_json 1、thiserror 2、base64 0.22、uuid 1（v4）、tauri-plugin-log 2.9
- 脚手架：`npm create tauri-app@latest`（React+TS+npm，项目名 phonectrl）

## Windows 构建策略
macOS 无法交叉编译到 Windows → 用 GitHub Actions 矩阵（macos-14 arm64 / windows-latest x64）分别出 .dmg/.exe；本地 `npm run tauri build` 出 macOS 包。Windows 端 SettingsDialog 提供 platform-tools 与 ffmpeg 下载指引。

## 实施里程碑
- **M0 脚手架**：create-tauri-app + 配置 + capabilities；`npm run tauri dev` 出窗口
- **M1 设备层**：config 探测、AdbClient、devices_list/props、DeviceBar + SettingsDialog；验证：2 台设备列表/详情正确，改路径生效
- **M2 投屏+控制**：pipeline、Channel 推帧、MirrorCanvas、坐标映射、input 全系、InputQueue；验证：双设备开流首帧<2s、延迟可感知<1s、点击/滑动/打字（ASCII+中文）正常、拔线自动停流
- **M3 快捷指令**：截屏/录屏/Home/返回/音量/电源/唤醒；验证：截图可打开、录 10s 可播放、按键生效
- **M4 文件管理**：浏览/上传/下载/删除/重命名/新建；验证：双设备操作、5MB 文件 push 后 pull md5 一致
- **M5 应用管理**：列表/安装/卸载/启动/停止；验证：装测试 APK、launch 拉起、force-stop 停止
- **M6 宏**：录制/保存/回放；验证：录"打开设置→返回"重启后回放成功
- **M7 打磨+CI**：日志、错误提示、release.yml；验证：CI 产出双平台安装包

## 验证（端到端自检命令）
上线前先在设备上手动验证管道：
```bash
adb -s MDX0220422024078 exec-out screenrecord --output-format=h264 --size 540x1172 --bit-rate 4000000 --time-limit 5 - \
  | /opt/homebrew/bin/ffmpeg -hide_banner -loglevel error -fflags nobuffer -flags low_delay -f h264 -i pipe:0 \
      -an -c:v mjpeg -q:v 4 -r 20 -f mjpeg -y /tmp/test.avi
# ffprobe 验证 mjpeg 流、时长 ~5s、首帧非全绿
```

## 已知限制（v1 接受）
- 安全 Surface（DRM/录屏禁令）显示黑屏；screenrecord 需每 170s 重启续流（~1s 间隙）
- `input` 单次调用 ~50-150ms，快速拖拽掉帧（终极方案 scrcpy-server 注入列入路线图）
- `ls -la` 解析含空格文件名失败；应用管理 v1 显示包名而非 label
- 双指捏合不支持；宏依赖同机型/方向/分辨率（回放前校验警告）
