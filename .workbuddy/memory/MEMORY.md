# phoneCtrl 项目长期笔记

## 架构
- Tauri 2 应用（src/ = React TS 前端，src-tauri/ = Rust 后端），scrcpy-server v4.0 投屏，h264 WebCodecs 播放
- 触摸注入通道优先级：sendevent 注入桥（直写 /dev/input，app_process 常驻）> scrcpy 控制通道（需显式 touchBackend="scrcpy"）> **motionevent 流式通道（motion.rs，持久 adb shell，2026-11-24 新增）**> adb 聚合（up 时重放 input swipe）
- 用户配置目录：~/Library/Application Support/com.phonectrl.desktop/（无 config.json = 默认 touch_backend="auto"）

## 设备关键事实（华为 ELS-AN00，P30 Pro，Android 12，serial MDX0220422024078）
- SELinux Enforcing：shell 写 /dev/input EACCES → sendevent 桥在此机型不可用
- `input motionevent DOWN|MOVE|UP` 可用（框架级注入），设备按 ~18ms/条节奏顺序执行，VelocityTracker 正常 → 原生 fling
- `input swipe` 注入事件不产生设备端惯性（拖拽跟随弱 + 松手戛然而止）——聚合通道的已知缺陷
- 屏幕分辨率 1080x2340，devicePixelRatio 3（遥测 px 为逻辑像素）

## 惯性方案共识
- 惯性本体交给设备端：事件流带真实时间戳，应用自身物理产生 fling
- 客户端不要合成指数衰减尾巴（末段速度≈0 抬起会掐灭设备端 fling）
- 唯一补偿：鼠标"停下再松开"（短停顿≤250ms 按停前速度补 5 步冲刺再抬起，sendRescueBurst）

## 关联项目
- 测试应用 chatDemo：~/Desktop/WorkSpace/FlutterProject/chatDemo（com.example.chat_demo），滚动遥测 = `--dart-define=SCROLL_TELEMETRY=true` 构建 + logcat -s flutter 过滤 `TEL *`
- 测试脚本模板：/tmp/inject_test.py（持久 shell motionevent 流式 + 遥测解析）
