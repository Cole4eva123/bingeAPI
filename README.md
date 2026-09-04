# bingeAPI

基于 Tauri 2、React 19、TypeScript 和 Rust 的桌面应用。项目使用 SQLite 保存渠道、访问密钥和请求日志，Rust 侧通过 SQLx 异步访问数据库。

## 数据库概览

- 数据库：SQLite
- Rust 数据库框架：SQLx 0.8
- 数据库文件：Tauri 应用数据目录下的 `bingeapi.db`
- 初始化脚本：[`init.sql`](./init.sql)
- 连接与初始化：[`src-tauri/src/db/mod.rs`](./src-tauri/src/db/mod.rs)
- Rust 数据模型：[`src-tauri/src/db/models.rs`](./src-tauri/src/db/models.rs)
- CRUD 与查询：[`src-tauri/src/db/repository.rs`](./src-tauri/src/db/repository.rs)

应用启动时会获取操作系统为当前应用分配的数据目录，创建 `bingeapi.db` 和最多包含 5 条连接的连接池，然后执行内嵌的 `init.sql`。脚本中的表和索引都使用 `IF NOT EXISTS`，因此重复启动不会重复创建。

当前数据库有三张表：

```text
channels
   │  id（由 request_logs.channel_id 逻辑引用）
   └──────────────┐
                  │
api_keys          │
   │  id          │
   └──────────┐   │
              ▼   ▼
          request_logs
```

这些关联目前只通过字段表达，`init.sql` **没有声明外键约束**。因此删除渠道或 API Key 时，SQLite 不会自动删除或修改对应日志。

### `channels`：上游渠道

| 字段 | SQLite 类型 | 约束/默认值 | 含义 |
| --- | --- | --- | --- |
| `id` | `TEXT` | 主键 | Rust 后端生成的 UUID |
| `name` | `TEXT` | 非空 | 渠道名称 |
| `type` | `TEXT` | 非空 | 渠道类型；Rust 中映射为 `channel_type` |
| `base_url` | `TEXT` | 非空 | 上游服务地址 |
| `api_key` | `TEXT` | 非空 | 上游 API Key |
| `models` | `TEXT` | 非空，默认 `[]` | JSON 数组字符串 |
| `status` | `INTEGER` | 非空，默认 `1` | 启用状态 |
| `priority` | `INTEGER` | 非空，默认 `0` | 渠道优先级，列表查询按其降序排列 |
| `weight` | `INTEGER` | 非空，默认 `1` | 调度权重 |
| `config` | `TEXT` | 非空，默认 `{}` | JSON 对象字符串，保存扩展配置 |
| `model_mapping` | `TEXT` | 非空，默认 `{}` | JSON 对象字符串，保存模型映射 |
| `created_at` | `TEXT` | 非空 | UTC ISO-8601 创建时间 |
| `updated_at` | `TEXT` | 非空 | UTC ISO-8601 更新时间 |
| `last_test_at` | `TEXT` | 可空 | 最近一次渠道测试时间 |
| `last_test_ok` | `INTEGER` | 可空 | 最近一次测试是否成功 |

### `api_keys`：客户端访问密钥

| 字段 | SQLite 类型 | 约束/默认值 | 含义 |
| --- | --- | --- | --- |
| `id` | `TEXT` | 主键 | API Key 记录 ID |
| `name` | `TEXT` | 非空 | 密钥名称 |
| `key` | `TEXT` | 非空、唯一 | 客户端实际使用的密钥 |
| `status` | `INTEGER` | 非空，默认 `1` | 启用状态 |
| `allowed_models` | `TEXT` | 非空，默认 `[]` | 允许访问的模型，JSON 数组字符串 |
| `allowed_channels` | `TEXT` | 非空，默认 `[]` | 允许访问的渠道，JSON 数组字符串 |
| `quota_limit` | `INTEGER` | 非空，默认 `-1` | 配额上限；当前业务代码尚未实现该值的判定逻辑 |
| `quota_used` | `INTEGER` | 非空，默认 `0` | 已使用配额 |
| `expires_at` | `TEXT` | 可空 | 过期时间 |
| `created_at` | `TEXT` | 非空 | UTC ISO-8601 创建时间 |
| `updated_at` | `TEXT` | 非空 | UTC ISO-8601 更新时间 |

### `request_logs`：请求日志

| 字段 | SQLite 类型 | 约束/默认值 | 含义 |
| --- | --- | --- | --- |
| `id` | `TEXT` | 主键 | 日志 ID |
| `api_key_id` | `TEXT` | 可空 | 关联的 API Key ID（无外键约束） |
| `api_key_name` | `TEXT` | 可空 | 请求发生时的 API Key 名称快照 |
| `channel_id` | `TEXT` | 可空 | 关联的渠道 ID（无外键约束） |
| `channel_name` | `TEXT` | 可空 | 请求发生时的渠道名称快照 |
| `model` | `TEXT` | 非空 | 客户端请求的模型 |
| `upstream_model` | `TEXT` | 可空 | 映射后的上游模型 |
| `mode` | `TEXT` | 非空 | 请求模式 |
| `status_code` | `INTEGER` | 非空 | HTTP 状态码 |
| `prompt_tokens` | `INTEGER` | 非空，默认 `0` | 输入 token 数 |
| `completion_tokens` | `INTEGER` | 非空，默认 `0` | 输出 token 数 |
| `total_tokens` | `INTEGER` | 非空，默认 `0` | token 总数 |
| `duration_ms` | `INTEGER` | 非空，默认 `0` | 请求耗时，单位毫秒 |
| `error_message` | `TEXT` | 可空 | 错误信息 |
| `is_stream` | `INTEGER` | 非空，默认 `0` | 是否为流式请求 |
| `is_retry` | `INTEGER` | 非空，默认 `0` | 是否为重试请求 |
| `created_at` | `TEXT` | 非空 | UTC ISO-8601 创建时间 |

SQLite 没有独立的布尔类型，项目使用 `INTEGER` 保存布尔状态，通常以 `0` 表示否、`1` 表示是。时间统一以 `TEXT` 保存；`models` 等结构化值则先序列化为 JSON，再存入 `TEXT`。

日志表已建立以下索引：

| 索引 | 字段 | 主要用途 |
| --- | --- | --- |
| `idx_logs_created` | `created_at` | 时间排序和时间范围查询 |
| `idx_logs_channel` | `channel_id` | 按渠道查询 |
| `idx_logs_api_key` | `api_key_id` | 按 API Key 查询 |
| `idx_logs_model` | `model` | 按模型查询 |

> 注意：Rust 的 `RequestLog` 模型还预留了 `seq`、`request_body` 和安全审计相关字段，但当前 `init.sql` 并没有创建这些列。`search_logs` 暂时通过 SQL 别名返回默认值以兼容该模型；上表才是目前实际创建的数据库结构。

## Rust 中如何使用 SQLx

SQLx 是异步 SQL 工具包，而不是会隐藏 SQL 的传统 ORM。项目仍然手写 SQL，并由 SQLx 负责连接池、参数绑定、异步执行以及查询结果到 Rust 结构体的映射。

### 1. 依赖配置

配置位于 `src-tauri/Cargo.toml`：

```toml
sqlx = { version = "0.8", default-features = false, features = [
  "sqlite",
  "runtime-tokio",
  "macros"
] }
```

- `sqlite`：启用 SQLite 驱动。
- `runtime-tokio`：让异步数据库操作运行在 Tokio/Tauri 异步运行时上。
- `macros`：启用 `FromRow` 等派生宏。

### 2. 创建数据库和连接池

`Database::new` 使用 Tauri 的 `AppHandle` 找到应用数据目录，并建立连接池：

```rust
let options = SqliteConnectOptions::new()
    .filename(&db_path)
    .create_if_missing(true);

let pool = SqlitePoolOptions::new()
    .max_connections(5)
    .connect_with(options)
    .await?;
```

连接池类型是 `SqlitePool`。它可以安全地被多个异步任务共享，业务代码不需要为每次查询重新打开数据库文件。

### 3. 执行初始化脚本

```rust
sqlx::raw_sql(include_str!("../../../init.sql"))
    .execute(&pool)
    .await?;
```

`include_str!` 在编译期将 `init.sql` 嵌入二进制；`raw_sql` 可以一次执行脚本中的多条建表和建索引语句。因此发布应用时不需要在数据库旁额外放置这份 SQL 文件。

### 4. 注册到 Tauri State

应用启动时创建一次数据库，并将它注册为全局状态：

```rust
let database = tauri::async_runtime::block_on(
    db::Database::new(&app_handle)
)?;

app.manage(database);
```

之后任意 Tauri command 都可以取得同一个连接池：

```rust
#[tauri::command]
async fn list_channels(
    state: tauri::State<'_, db::Database>,
) -> Result<Vec<Channel>, String> {
    repository::list_channels(&state.pool).await
}
```

整体调用链为：

```text
React invoke(...)
  → Tauri command
  → tauri::State<Database>
  → repository 函数
  → SqlitePool
  → bingeapi.db
```

当前代码已经把渠道的查询、新增、部分更新和删除暴露为 Tauri command；API Key 目前只有按启用状态查询的 repository 函数；日志搜索也已写在 repository 中，但尚未注册为前端可调用的 command。

### 5. 将查询结果映射为 Rust 结构体

数据模型派生 `sqlx::FromRow`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Channel {
    pub id: String,
    pub name: String,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub channel_type: String,
    // ...
}
```

然后通过 `query_as` 将每一行直接映射为 `Channel`：

```rust
let channels = sqlx::query_as::<_, Channel>(
    "SELECT id, name, \"type\", base_url, api_key, models, status, \
     priority, weight, config, model_mapping, created_at, updated_at, \
     last_test_at, last_test_ok FROM channels"
)
.fetch_all(pool)
.await?;
```

常用读取方法：

- `fetch_one`：必须读取一行，没有记录时返回错误。
- `fetch_optional`：读取零或一行，返回 `Option<T>`。
- `fetch_all`：读取多行，返回 `Vec<T>`。
- `execute`：执行 `INSERT`、`UPDATE`、`DELETE` 等不需要映射结果行的语句。

### 6. 使用参数绑定

所有外部输入都应通过 `.bind(...)` 或 `QueryBuilder::push_bind(...)` 绑定：

```rust
sqlx::query("DELETE FROM channels WHERE id = ?")
    .bind(id)
    .execute(pool)
    .await?;
```

不要使用 `format!` 把用户输入直接拼入 SQL。参数绑定既能避免 SQL 注入，也会让 SQLx 正确处理字符串转义和类型转换。项目的日志搜索使用 `QueryBuilder<Sqlite>` 动态追加可选条件，同时仍通过 `push_bind` 绑定所有值。

### 7. JSON、可空值与错误处理

- 前端传入的 `Vec<String>`、`serde_json::Value` 会先由 `serde_json::to_string` 转为 JSON 文本，再写入 SQLite。
- 数据库中的可空字段在 Rust 中使用 `Option<T>`，例如 `Option<String>`。
- repository 统一返回 `Result<T, String>`，使用 `map_err` 将 SQLx 错误转换成可直接返回给 Tauri 前端的字符串。
- `CreateChannelInput` 和 `UpdateChannelInput` 是输入 DTO；数据库模型与前端输入分开后，后端可以自行生成 UUID、时间和默认值。

### 8. 添加一次新的数据库操作

通常按以下顺序开发：

1. 在 `init.sql` 中添加或调整表结构。
2. 在 `models.rs` 中添加数据库模型和输入 DTO。
3. 在 `repository.rs` 中使用 `query`、`query_as` 或 `QueryBuilder` 编写数据库操作。
4. 在 `lib.rs` 中添加 `#[tauri::command]`，从 `State` 取得连接池并调用 repository。
5. 将 command 加入 `tauri::generate_handler![...]`。
6. 前端通过 `@tauri-apps/api/core` 的 `invoke` 调用。

前端示例：

```ts
import { invoke } from "@tauri-apps/api/core";

const channels = await invoke("list_channels");

const created = await invoke("create_channel", {
  input: {
    name: "OpenAI",
    type: "openai",
    base_url: "https://api.openai.com/v1",
    api_key: "...",
    models: ["gpt-4o"],
    priority: 0,
    weight: 1,
  },
});
```

## 表结构变更注意事项

当前项目使用的是“启动时执行初始化脚本”，尚未接入 SQLx migrations。`CREATE TABLE IF NOT EXISTS` 只能创建不存在的表，**不会为已经存在的表自动增加新字段**。如果后续修改表结构，建议引入 `sqlx::migrate!()` 和按版本编号的迁移文件；否则开发期需要手动迁移已有的 `bingeapi.db`。

此外，`channels.api_key` 和 `api_keys.key` 当前以明文保存在本地 SQLite 中。若应用要用于生产环境，应根据威胁模型考虑系统钥匙串、加密存储或只保存不可逆哈希，并避免在日志和错误信息中输出密钥。

## 开发运行

安装依赖并启动 Tauri 应用：

```bash
bun install
bun run tauri dev
```

只启动 Vite 前端：

```bash
bun run dev
```
