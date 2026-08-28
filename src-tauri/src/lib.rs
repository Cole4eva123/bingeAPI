mod db;

use db::models::{Channel, CreateChannelInput, UpdateChannelInput};
use db::repository;
use sqlx::Row;
use tauri::Manager;

/// Tauri command 示例：验证前端到 Rust 的调用链是否正常。
///
/// 前端可以通过 `invoke("greet", { name: "xxx" })` 调用这个函数。
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 返回 channels 表中的记录数量。
///
/// 这个 command 主要用于验证 SQLite 连接是否成功：
/// 1. Tauri 从 State 中取出 `db::Database`；
/// 2. SQLx 从连接池中取得一条连接；
/// 3. 执行 COUNT 查询；
/// 4. 将查询结果返回给前端。
///
/// `async` 是因为 SQLx 的数据库操作是异步的，不能阻塞 Tauri 的运行时。
/// `State<'_, db::Database>` 中的生命周期由 Tauri 管理，不需要手动释放。
#[tauri::command]
async fn count_channels(state: tauri::State<'_, db::Database>) -> Result<i64, String> {
    // query 用于执行普通 SQL。这里没有用户输入，所以不需要绑定参数；
    // 带用户输入的 SQL 必须使用 bind/参数绑定，不能拼接字符串。
    let row = sqlx::query("SELECT COUNT(*) AS count FROM channels")
        .fetch_one(&state.pool)
        .await
        .map_err(|error| error.to_string())?;

    // 根据 SQL 中的别名 "count" 读取第一行的 count 列。
    // try_get 失败时，将 SQLx 错误转换为 Tauri command 可以返回的 String。
    row.try_get("count").map_err(|error| error.to_string())
}

/// 查询所有渠道。
///
/// 前端调用：`invoke("list_channels")`。
#[tauri::command]
async fn list_channels(state: tauri::State<'_, db::Database>) -> Result<Vec<Channel>, String> {
    repository::list_channels(&state.pool).await
}

/// 根据 ID 查询单个渠道。
///
/// 不存在时返回 `null`，而不是把“不存在”当成数据库错误。
/// 前端调用：`invoke("get_channel", { id })`。
#[tauri::command]
async fn get_channel(
    state: tauri::State<'_, db::Database>,
    id: String,
) -> Result<Option<Channel>, String> {
    repository::find_channel(&state.pool, &id).await
}

/// 新增渠道。
///
/// `CreateChannelInput` 会由 Tauri 根据前端传入的 JSON 自动反序列化。
/// ID、创建时间和更新时间由 Rust 后端生成，前端不需要传入。
#[tauri::command]
async fn create_channel(
    state: tauri::State<'_, db::Database>,
    input: CreateChannelInput,
) -> Result<Channel, String> {
    repository::create_channel(&state.pool, input).await
}

/// 部分更新渠道。
///
/// `UpdateChannelInput` 中为 `None` 的字段保持数据库原值，
/// 传入 `Some(value)` 的字段才会被更新。
#[tauri::command]
async fn update_channel(
    state: tauri::State<'_, db::Database>,
    input: UpdateChannelInput,
) -> Result<Channel, String> {
    repository::update_channel(&state.pool, input).await
}

/// 删除渠道。
///
/// 删除成功返回 `true`；如果 ID 不存在，repository 会返回错误信息。
#[tauri::command]
async fn delete_channel(state: tauri::State<'_, db::Database>, id: String) -> Result<bool, String> {
    repository::delete_channel(&state.pool, &id).await?;
    Ok(true)
}

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
            greet,
            count_channels,
            list_channels,
            get_channel,
            create_channel,
            update_channel,
            delete_channel
        ])
        .run(tauri::generate_context!())
        // run 返回错误时终止程序，并输出失败原因。
        .expect("error while running tauri application");
}
