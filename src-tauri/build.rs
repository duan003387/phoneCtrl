fn main() {
    // 内置离线 Appium 运行时是构建产物（由 scripts/bundle-appium.sh 生成、且不入库）。
    // 若缺失，这里补一个 0 字节占位，保证 bundle.resources 引用它时构建不失败；
    // 运行时若占位无效会自动回退到全局/npx 的 Appium。
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    let bundle = dir.join("appium-bundle.tar.gz");
    if !bundle.exists() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&bundle, []);
    }
    tauri_build::build()
}
