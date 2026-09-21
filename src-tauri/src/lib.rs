mod adb;
mod commands;
mod config;
mod error;
mod macros;
mod state;
mod stream;
mod util;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            // 记录资源目录（打包后为 Contents/Resources），供 adb / jar 资源解析。
            if let Ok(rd) = app.path().resource_dir() {
                util::set_resource_dir(rd);
            }
            app.manage(tauri::async_runtime::block_on(AppState::new(config_dir)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 设备
            commands::devices_list,
            commands::device_start_server,
            commands::device_props,
            // 设置
            commands::settings_get,
            commands::settings_set,
            commands::diagnostics,
            // 投屏
            commands::stream_start,
            commands::stream_stop,
            commands::stream_status,
            commands::stream_request_keyframe,
            // 输入
            commands::input_tap,
            commands::input_swipe,
            commands::input_key,
            commands::input_text,
            commands::input_touch,
            // 文件
            commands::files_list,
            commands::files_delete,
            commands::files_mkdir,
            commands::files_rename,
            commands::files_copy,
            commands::files_upload,
            commands::files_upload_bytes,
            commands::files_download,
            commands::files_read,
            // 应用
            commands::apps_list,
            commands::apps_install,
            commands::apps_install_bytes,
            commands::apps_uninstall,
            commands::apps_launch,
            commands::apps_stop,
            commands::apps_info,
            // 快捷指令
            commands::action_screenshot,
            commands::action_screenshot_save,
            commands::action_record,
            commands::action_key,
            // 宏
            commands::macro_save,
            commands::macro_list,
            commands::macro_delete,
            commands::macro_play,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
