//! 应用组装入口：初始化共享资源、注册 command、启动 Tauri。
mod adapter;
mod commands;
mod db;
mod error;
mod models;
mod services;

use std::time::Duration;
use tauri::Manager;

pub(crate) struct AppState {
    database: db::Database,
    http_client: reqwest::Client,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database =
                tauri::async_runtime::block_on(db::Database::open(&data_dir.join("bingeapi.db")))?;
            // 仅设置连接超时，避免总超时截断较长的 SSE 响应。
            let http_client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()?;
            app.manage(AppState {
                database,
                http_client,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::count_channels,
            commands::list_channels,
            commands::get_channel,
            commands::test_channel,
            commands::create_channel,
            commands::update_channel,
            commands::delete_channel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
