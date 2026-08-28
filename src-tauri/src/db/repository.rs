//! 数据库 CRUD repository。
//!
//! repository 层只负责和数据库交互，不负责处理 Tauri 的 State 或前端
//! 参数。这样做的好处是：
//! - SQL 集中在一个地方，后续修改表结构更容易；
//! - repository 可以单独测试；
//! - Tauri command 只负责取 State、调用 repository、返回结果。

use serde::Serialize;
use sqlx::{query, query_as, QueryBuilder, Sqlite, SqlitePool};
use uuid::Uuid;

use super::models::{now_iso, ApiKey, Channel, CreateChannelInput, RequestLog, UpdateChannelInput};

/// repository 层统一返回字符串错误，方便直接作为 Tauri command 的错误返回值。
pub type RepositoryResult<T> = Result<T, String>;

/// channels 查询需要的列。
///
/// 不使用 `SELECT *` 是为了避免将来给表增加字段后，查询结果和 Rust
/// 结构体的字段不一致。`type` 用双引号包起来，避免和 SQL 关键字产生歧义。
const CHANNEL_COLUMNS: &str = r#"
    id,
    name,
    "type",
    base_url,
    api_key,
    models,
    status,
    priority,
    weight,
    config,
    model_mapping,
    created_at,
    updated_at,
    last_test_at,
    last_test_ok
"#;

/// 将 Rust 数据序列化为 JSON 字符串，存入 SQLite 的 TEXT 字段。
///
/// 你的 init.sql 中 `models`、`config` 和 `model_mapping` 都是 TEXT，
/// 所以它们需要在写入数据库前转换成 JSON 字符串。
fn to_json<T: Serialize>(value: &T) -> RepositoryResult<String> {
    serde_json::to_string(value).map_err(|error| format!("failed to serialize JSON: {error}"))
}

/// 查询单个渠道。
pub async fn find_channel(pool: &SqlitePool, id: &str) -> RepositoryResult<Option<Channel>> {
    let sql = format!("SELECT {CHANNEL_COLUMNS} FROM channels WHERE id = ?");

    query_as::<_, Channel>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to find channel: {error}"))
}

/// 查询所有渠道。
pub async fn list_channels(pool: &SqlitePool) -> RepositoryResult<Vec<Channel>> {
    let sql = format!(
        "SELECT {CHANNEL_COLUMNS} FROM channels \
         ORDER BY priority DESC, created_at DESC"
    );

    query_as::<_, Channel>(&sql)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to list channels: {error}"))
}

/// 新增渠道，并返回插入后的完整记录。
pub async fn create_channel(
    pool: &SqlitePool,
    input: CreateChannelInput,
) -> RepositoryResult<Channel> {
    // id 不从前端接收，避免前端传入重复 ID；由后端统一生成 UUID。
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    // 前端传 Vec<String>，数据库存 JSON 字符串，例如 ["gpt-4o", "claude-3"]。
    let models = to_json(&input.models)?;
    let config = to_json(&input.config.unwrap_or_else(|| serde_json::json!({})))?;
    let model_mapping = to_json(&input.model_mapping.unwrap_or_else(|| serde_json::json!({})))?;
    //如果 input.config 是 Some(...)，就把里面的值取出来；如果是 None，就执行后面的闭包生成一个默认值。也就是|| serde_json::json!({})
    //闭包就是没有名字的小函数，可以直接写在需要它的地方

    // 使用参数绑定 `?`，不要用 format! 拼接用户输入，避免 SQL 注入。
    query(
        r#"
        INSERT INTO channels (
            id, name, "type", base_url, api_key, models,
            status, priority, weight, config, model_mapping,
            created_at, updated_at, last_test_at, last_test_ok
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)
        "#,
    )
    .bind(&id)
    .bind(input.name)
    .bind(input.channel_type)
    .bind(input.base_url)
    .bind(input.api_key)
    .bind(models)
    .bind(1_i64) // 新增渠道默认启用
    .bind(input.priority.unwrap_or(0))
    .bind(input.weight.unwrap_or(1))
    .bind(config)
    .bind(model_mapping)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to create channel: {error}"))?;

    // 插入成功后再查询一次，返回数据库中的完整记录。
    find_channel(pool, &id)
        .await?
        .ok_or_else(|| "channel was inserted but could not be loaded".to_string())
}

/// 部分更新渠道。
///
/// UpdateChannelInput 中为 None 的字段保持原值；Some(value) 才会更新。
pub async fn update_channel(
    pool: &SqlitePool,
    input: UpdateChannelInput,
) -> RepositoryResult<Channel> {
    let models = input.models.as_ref().map(to_json).transpose()?;
    let config = input.config.as_ref().map(to_json).transpose()?;
    let model_mapping = input.model_mapping.as_ref().map(to_json).transpose()?;
    let now = now_iso();

    // COALESCE(?, column) 的含义是：参数有值就用参数，没有值就保留原列。
    let result = query(
        r#"
        UPDATE channels SET
            name = COALESCE(?, name),
            "type" = COALESCE(?, "type"),
            base_url = COALESCE(?, base_url),
            api_key = COALESCE(?, api_key),
            models = COALESCE(?, models),
            status = COALESCE(?, status),
            priority = COALESCE(?, priority),
            weight = COALESCE(?, weight),
            config = COALESCE(?, config),
            model_mapping = COALESCE(?, model_mapping),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(input.name)
    .bind(input.channel_type)
    .bind(input.base_url)
    .bind(input.api_key)
    .bind(models)
    .bind(input.status)
    .bind(input.priority)
    .bind(input.weight)
    .bind(config)
    .bind(model_mapping)
    .bind(now)
    .bind(&input.id)
    .execute(pool)
    .await
    .map_err(|error| format!("failed to update channel: {error}"))?;

    if result.rows_affected() == 0 {
        return Err(format!("channel not found: {}", input.id));
    }

    find_channel(pool, &input.id)
        .await?
        .ok_or_else(|| "channel was updated but could not be loaded".to_string())
}

/// 删除渠道。
pub async fn delete_channel(pool: &SqlitePool, id: &str) -> RepositoryResult<()> {
    let result = query("DELETE FROM channels WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to delete channel: {error}"))?;

    if result.rows_affected() == 0 {
        return Err(format!("channel not found: {id}"));
    }

    Ok(())
}

/// 根据 API Key 查询一个处于启用状态的密钥。
///
/// `fetch_optional` 的返回值是 `Option<ApiKey>`：
/// - 找到密钥：`Ok(Some(api_key))`
/// - 不存在或已禁用：`Ok(None)`
/// - 数据库查询失败：`Err(message)`
pub async fn get_api_key_by_key(pool: &SqlitePool, key: &str) -> RepositoryResult<Option<ApiKey>> {
    query_as::<_, ApiKey>(
        //query_as能把查询结果自动转化为Rust结构体
        r#"
        SELECT
            id,
            name,
            "key",
            status,
            allowed_models,
            allowed_channels,
            quota_limit,
            quota_used,
            expires_at,
            created_at,
            updated_at
        FROM api_keys
        WHERE "key" = ?
          AND status = 1
        "#,
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to get api key: {error}"))
}

pub async fn search_logs(
    pool: &SqlitePool,
    keyword: Option<&str>,
    channel_name: Option<&str>,
    model: Option<&str>,
    date_from: Option<&str>,
    date_to: Option<&str>,
    limit: i64,  //分页用
    offset: i64, //分页用
) -> RepositoryResult<Vec<RequestLog>> {
    // RequestLog 模型中包含一些当前 init.sql 尚未创建的字段。这里为这些字段
    // 返回兼容默认值，让 FromRow 可以完整构造 RequestLog：
    // - 可选字段使用 NULL；
    // - 必填数字字段使用 0；
    // - 必填文本字段使用安全的默认字符串。
    // 等以后真正给 request_logs 增加这些列后，再把默认值替换为对应列名。
    let mut q = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            id,
            NULL AS seq,
            api_key_id,
            api_key_name,
            channel_id,
            channel_name,
            model,
            upstream_model,
            mode,
            status_code,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            duration_ms,
            error_message,
            is_stream,
            is_retry,
            created_at,
            NULL AS request_body,
            'low' AS risk_level,
            0 AS risk_score,
            NULL AS risk_summary,
            'allow' AS security_action,
            0 AS sanitized,
            NULL AS blocked_reason
        FROM request_logs
        WHERE 1 = 1
        "#,
    );

    // 模糊搜索。LIKE 两边的 % 表示关键词前后可以出现任意字符。
    // 使用 push_bind 绑定参数，避免把用户输入直接拼接进 SQL。
    if let Some(kw) = keyword.map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = format!("%{kw}%");

        q.push(" AND (")
            .push("COALESCE(api_key_name, '') LIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(channel_name, '') LIKE ")
            .push_bind(pattern.clone())
            .push(" OR model LIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(upstream_model, '') LIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(error_message, '') LIKE ")
            .push_bind(pattern)
            .push(")");
    }

    // 按渠道名称精确筛选。
    if let Some(name) = channel_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        q.push(" AND channel_name = ").push_bind(name);
    }

    // 按请求模型精确筛选。
    if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) {
        q.push(" AND model = ").push_bind(model);
    }

    // created_at 使用 ISO-8601 文本保存。同一格式下可以直接按字符串比较时间。
    if let Some(from) = date_from.map(str::trim).filter(|value| !value.is_empty()) {
        q.push(" AND created_at >= ").push_bind(from);
    }

    if let Some(to) = date_to.map(str::trim).filter(|value| !value.is_empty()) {
        q.push(" AND created_at <= ").push_bind(to);
    }

    // 新日志排在前面。限制每页最多 1000 条，并确保 offset 不会为负数。
    let safe_limit = limit.clamp(1, 1000);
    let safe_offset = offset.max(0);

    q.push(" ORDER BY created_at DESC")
        .push(" LIMIT ")
        .push_bind(safe_limit)
        .push(" OFFSET ")
        .push_bind(safe_offset);

    // build_query_as 会使用 RequestLog 的 FromRow 实现，把每行自动转换为
    // RequestLog；fetch_all 表示返回当前分页中的全部记录。
    q.build_query_as::<RequestLog>()
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to search request logs: {error}"))
}