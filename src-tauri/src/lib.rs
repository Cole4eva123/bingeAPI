//! Tauri 应用入口模块。
//!
//! 本文件只负责两件事：
//! 1. 声明应用的顶层模块（`db`、`adaptor`、`commands`）；
//! 2. 在 `run()` 中完成 Tauri 应用的初始化和启动。
//!
//! 所有 `#[tauri::command]` 函数定义在 `commands` 模块中，
//! 数据库初始化在 `db` 模块中，上游适配器在 `adaptor` 模块中。

/// 数据库模块——连接池初始化、数据模型和 CRUD repository。
mod db;

/// 上游渠道适配器模块——`Adaptor` trait 及各供应商实现（OpenAI、DeepSeek 等）。
mod adaptor;

/// Tauri command 模块——前端 `invoke(...)` 的入口函数。
mod commands;

use tauri::Manager;

/// 启动 Tauri 应用。
///
/// 这个函数完成以下步骤：
/// 1. 注册插件（`tauri_plugin_opener`）；
/// 2. 在 `setup` 中初始化数据库并注册为全局 State；
/// 3. 通过 `generate_handler!` 注册所有前端可调用的 command；
/// 4. 启动事件循环。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // AppHandle 可以安全地复制并传给异步初始化函数。
            // clone 的是句柄，不是数据库连接，也不会复制数据库文件。
            let app_handle = app.handle().clone();

            // setup 闭包本身是同步的，而 Database::new 是 async 函数。
            // block_on 会等待数据库连接池创建和 init.sql 执行完毕，
            // 确保窗口和 command 开始工作前数据库已经准备好。
            let database = tauri::async_runtime::block_on(db::Database::new(&app_handle))
                .map_err(std::io::Error::other)?;

            // 将 Database 注册为全局 Tauri State。
            // 注册后，任意带有 State<db::Database> 参数的 command 都能访问
            // 同一个连接池。
            app.manage(database);
            Ok(())
        })
        // 只有放进 generate_handler! 的函数才能从前端 invoke 调用。
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
        // run 返回错误时终止程序，并输出失败原因。
        .expect("error while running tauri application");
}
