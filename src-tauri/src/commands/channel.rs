//! 渠道管理相关的 Tauri command。
//!
//! 这些函数通过 `#[tauri::command]` 暴露给前端，前端可以用
//! `invoke("command_name", { ... })` 调用。每个函数只负责：
//! 1. 从 Tauri State 取出数据库连接池；
//! 2. 调用 `repository` 层执行具体的数据库操作；
//! 3. 返回结果给前端。

use crate::adaptor::{get_adaptor, ChannelConfig, TestResult};
use crate::db;
use crate::db::models::{Channel, CreateChannelInput, UpdateChannelInput};
use crate::db::repository;

/// Tauri command 示例：验证前端到 Rust 的调用链是否正常。
///
/// 前端可以通过 `invoke("greet", { name: "xxx" })` 调用这个函数。
#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// 返回 channels 表中的记录数量。
///
/// 这个 command 主要用于验证 SQLite 连接是否成功。
/// 通过 repository 层执行 SQL 查询，保持分层一致性。
///
/// `async` 是因为 SQLx 的数据库操作是异步的，不能阻塞 Tauri 的运行时。
/// `State<'_, db::Database>` 中的生命周期由 Tauri 管理，不需要手动释放。
#[tauri::command]
pub async fn count_channels(state: tauri::State<'_, db::Database>) -> Result<i64, String> {
    repository::count_channels(&state.pool).await
}

/// 查询所有渠道。
///
/// 前端调用：`invoke("list_channels")`。
#[tauri::command]
pub async fn list_channels(state: tauri::State<'_, db::Database>) -> Result<Vec<Channel>, String> {
    repository::list_channels(&state.pool).await
}

/// 根据 ID 查询单个渠道。
///
/// 不存在时返回 `null`，而不是把"不存在"当成数据库错误。
/// 前端调用：`invoke("get_channel", { id })`。
#[tauri::command]
pub async fn get_channel(
    state: tauri::State<'_, db::Database>,
    id: String,
) -> Result<Option<Channel>, String> {
    repository::find_channel(&state.pool, &id).await
}

/// 使用数据库中保存的地址、API Key 和模型测试渠道连通性。
///
/// 前端调用：`invoke("test_channel", { id })`。
#[tauri::command]
pub async fn test_channel(
    state: tauri::State<'_, db::Database>,
    id: String,
) -> Result<TestResult, String> {
    // find_channel 返回 Option；渠道不存在时转换成人类可读的错误。
    let channel = repository::find_channel(&state.pool, &id)
        .await?
        .ok_or_else(|| format!("channel not found: {id}"))?;

    // 数据库使用 TEXT 保存 JSON，适配器需要真正的 Rust 集合和 JSON Value。
    // 解析失败时直接报告具体字段，避免用空配置悄悄发出错误请求。
    let models = serde_json::from_str(&channel.models)
        .map_err(|error| format!("invalid channel models JSON: {error}"))?;
    let model_mapping = serde_json::from_str(&channel.model_mapping)
        .map_err(|error| format!("invalid channel model_mapping JSON: {error}"))?;
    let extra = serde_json::from_str(&channel.config)
        .map_err(|error| format!("invalid channel config JSON: {error}"))?;

    let config = ChannelConfig {
        base_url: channel.base_url,
        api_key: channel.api_key,
        models,
        model_mapping,
        extra,
    };

    // 根据数据库中的 type 创建 deepseek、openai 等对应适配器。
    let adaptor = get_adaptor(&channel.channel_type).map_err(|error| error.to_string())?;
    let result = adaptor
        .test(&config)
        .await
        .map_err(|error| error.to_string())?;

    // 保存本次结果，供渠道列表展示最近测试状态。
    repository::update_channel_test_result(&state.pool, &id, result.success).await?;

    Ok(result)
}

/// 新增渠道。
///
/// `CreateChannelInput` 会由 Tauri 根据前端传入的 JSON 自动反序列化。
/// ID、创建时间和更新时间由 Rust 后端生成，前端不需要传入。
#[tauri::command]
pub async fn create_channel(
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
pub async fn update_channel(
    state: tauri::State<'_, db::Database>,
    input: UpdateChannelInput,
) -> Result<Channel, String> {
    repository::update_channel(&state.pool, input).await
}

/// 删除渠道。
///
/// 删除成功返回 `true`；如果 ID 不存在，repository 会返回错误信息。
#[tauri::command]
pub async fn delete_channel(
    state: tauri::State<'_, db::Database>,
    id: String,
) -> Result<bool, String> {
    repository::delete_channel(&state.pool, &id).await?;
    Ok(true)
}
