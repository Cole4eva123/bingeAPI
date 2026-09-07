# LLMAPI · bingeAPI 项目说明

本文件是项目主文档，兼作阅读源码时的 Rust 速查笔记。内容以当前代码和迁移脚本为准。

阅读顺序：先看 Rust 基础，再看项目分层和调用流程，最后查接口与数据库字段。

- [Rust 阅读指南](#rust-阅读指南)
- [项目概况与目录](#项目概况与目录)
- [程序逻辑与调用流程](#程序逻辑与调用流程)
- [前后端接口](#前后端接口)
- [适配器与流式响应](#适配器与流式响应)
- [数据库结构](#数据库结构)
- [开发与测试](#开发与测试)

## Rust 阅读指南

### 1. 工具各做什么

| 工具 | 用途 |
| --- | --- |
| `rustup` | 安装和管理 Rust 工具链 |
| `rustc` | 编译 Rust |
| `cargo` | 管理依赖、构建、运行和测试 |
| `rustfmt` | 格式化源码，通过 `cargo fmt` 使用 |
| `clippy` | 检查代码问题，通过 `cargo clippy` 使用 |

编辑器中，`rust-analyzer` 提供补全、类型提示、跳转和诊断；TOML 扩展用于编辑 `Cargo.toml`。Tauri 是桌面应用框架，Tauri 插件用于扩展系统能力，与编辑器扩展不同。

### 2. 变量、集合与返回值

```rust
let count = 3;                 // 默认不可重新赋值
let mut total = 0;             // mut 允许修改
total += count;

let fixed = [0; 3];           // 固定长度数组：[0, 0, 0]
let mut models = vec!["a"];   // Vec：可增长的集合
models.push("b");

fn add(a: i32, b: i32) -> i32 {
    a + b                    // 末尾无分号的表达式作为返回值
}
```

`[T; N]` 是固定长度数组，`Vec<T>` 是动态集合，`&[T]` 是借用一段连续元素的切片。提前返回时写 `return value;`；函数返回 `()` 表示没有有用的返回数据。

### 3. 所有权、借用与字符串

| 写法 | 含义 | 项目中的例子 |
| --- | --- | --- |
| `T` | 按值传递；通常移动所有权，`Copy` 类型会复制 | 接收 `CreateChannelInput` 后取出字段 |
| `&T` | 只读借用 | `&SqlitePool`、`&ChannelConfig` |
| `&mut T` | 可修改的借用 | 修改请求体中的 JSON 对象 |
| `String` | 拥有文本内容 | 保存渠道名称、密钥 |
| `&str` | 借用文本片段 | 函数接收渠道 ID 或模型名 |
| `&'static str` | 整个程序运行期间有效的文本引用 | 供应商名称、错误代码 |

```rust
let name = String::from("OpenAI");
let borrowed: &str = &name;   // 借用，name 仍然有效
let copied = name.clone();   // 复制文本，得到独立 String
let moved = name;            // 移动；之后不能再使用 name
```

字符串字面量 `"openai"` 的类型是 `&'static str`。转为 `String` 可用 `.to_string()`、`String::from(...)` 或在目标类型明确时用 `.into()`。

`clone()` 的开销由类型决定：`String` 会复制内容；本项目的 `reqwest::Client::clone()` 共享内部连接池。

### 4. Option：可能没有值

`Option<T>` 只有两种状态：`Some(value)` 有值，`None` 无值。`Some(18)` 的类型是 `Option<i32>`，不是 `i32`。

```rust
let name: Option<String> = Some("demo".into());

match name {
    Some(value) => println!("{value}"),
    None => println!("没有名称"),
}
```

只处理有值的情况时，用 `if let`：

```rust
if let Some(name) = &input.name {
    non_empty("name", name)?;
}
```

这里的 `&input.name` 避免移动字段；`name` 是模式匹配取出的引用，类型为 `&String`。

| 方法 | 用途 |
| --- | --- |
| `map(f)` | 有值时转换，结果仍包在 `Option` 中 |
| `and_then(f)` | 有值时继续一个返回 `Option` 的操作，避免嵌套 |
| `unwrap_or(value)` | 无值时使用默认值；默认表达式会先求值 |
| `unwrap_or_else(f)` | 无值时才调用闭包计算默认值 |
| `ok_or_else(f)` | 把 `Option<T>` 转成 `Result<T, E>` |
| `as_ref()` | 把 `Option<T>` 借用为 `Option<&T>` |
| `as_deref()` | 例如把 `Option<String>` 借用为 `Option<&str>` |
| `transpose()` | 把 `Option<Result<T, E>>` 转为 `Result<Option<T>, E>` |

项目中的链式写法：

```rust
if let Some(bytes) = request.body().and_then(|body| body.as_bytes()) {
    println!("{}", String::from_utf8_lossy(bytes));
}
```

含义：先检查有没有请求体，再检查能否直接取得字节；两步都有值才打印。流式请求体不一定能直接取得内存字节。

### 5. Result 与问号：处理失败

`Option` 表示“有没有”，`Result<T, E>` 表示“成功还是失败”：

```rust
Ok(value)   // 成功，携带数据
Err(error)  // 失败，携带原因
```

在返回 `Result` 的函数中，`?` 遇到 `Ok` 取出值，遇到 `Err` 提前返回；需要时通过 `From` 转换错误类型。

```rust
let channel = repository::find_channel(pool, id)
    .await?
    .ok_or_else(|| AppError::NotFound(id.to_owned()))?;
```

这两次 `?` 分别处理：数据库查询失败，以及查询成功但渠道不存在。

`Option` 和 `Result` 本身也能组合：

| 查询结果 | 含义 |
| --- | --- |
| `Ok(Some(channel))` | 查询成功，找到渠道 |
| `Ok(None)` | 查询成功，没有该渠道 |
| `Err(error)` | 查询过程失败 |

在返回 `Option` 的函数中，`?` 遇到 `None` 会提前返回 `None`。

`unwrap()` 和 `expect("说明")` 遇到无值或错误会触发 panic。项目主要在测试断言和无法继续启动的最终入口使用它们。

### 6. 闭包与迭代器

闭包是可以直接传给其他函数的小函数：`|参数| 表达式`。`||` 表示没有参数。

```rust
let add = |a, b| a + b;
let weight = input.weight.unwrap_or_else(|| 1);

let names: Vec<String> = channels
    .into_iter()
    .map(|channel| channel.info.name)
    .collect();
```

- `iter()` 借用元素；`into_iter()` 在这里消费集合，取得元素所有权。
- `map` 转换每一项；`filter` 保留满足条件的项；`collect` 收集结果。
- `|` 在闭包外也可能是按位或，或模式匹配中的“或”，例如 `matches!(status, 0 | 1)`。

### 7. struct、impl、trait 与 Box

```rust
struct Channel {
    name: String,
}

impl Channel {
    fn new(name: String) -> Self { Self { name } } // 关联函数
    fn name(&self) -> &str { &self.name }          // 方法
}

let channel = Channel::new("demo".into());
println!("{}", channel.name());
```

`Self` 表示当前实现的类型。`Channel::new(...)` 不需要现成实例；`channel.name()` 带有 `self` 接收者，等价于这里的 `Channel::name(&channel)`。

`trait` 定义一组能力，类型通过 `impl Trait for Type` 实现它：

```rust
trait Summary {
    fn summarize(&self) -> String;
}

fn notify(item: &impl Summary) {
    println!("{}", item.summarize());
}

// 对这个单参数例子，下面是等价的泛型写法。
fn notify_generic<T: Summary>(item: &T) {
    println!("{}", item.summarize());
}
```

项目使用 `Box<dyn Adapter>`：

- `Box<T>` 拥有堆上的值；`Box::new(10)` 把整数放入 Box。
- `dyn Adapter` 表示实现了 `Adapter` 的某种类型，调用时通过动态分派选择方法。
- `Box<dyn Adapter>` 让上层使用统一类型，不依赖具体适配器结构。

当前 OpenAI、DeepSeek 都由 `OpenAiCompatible` 配合不同预设实现；以后遇到不同协议，可新增另一种 `Adapter` 实现。

### 8. mod、use 与可见性

| 写法 | 含义 |
| --- | --- |
| `mod channel;` | 声明子模块，通常对应 `channel.rs` 或 `channel/mod.rs` |
| `use crate::models::channel::Channel;` | 从当前 crate 根路径引入类型 |
| `use super::now_iso;` | 从父模块引入 |
| `use crate::db::channel as repository;` | 给模块取本地别名，没有创建新模块 |
| `pub` | 对外可见，但仍受父模块可见性限制 |
| `pub(crate)` | 当前 crate 内可见 |
| `pub(super)` | 父模块范围内可见 |
| `pub use channel::*;` | 重新导出子模块中的公开项 |

一个 crate 是 Rust 的编译单元。本项目的 `src-tauri` package 同时包含二进制入口 `main.rs` 和库入口 `lib.rs`。

### 9. derive、Debug 与序列化

`#[...]` 是属性；`#[derive(...)]` 自动生成常见 trait 的实现。

| 派生项 | 作用 |
| --- | --- |
| `Debug` | 支持 `{:?}` 调试输出 |
| `Clone` | 支持 `.clone()` |
| `Default` | 提供默认值 |
| `Serialize` | 把 Rust 值转换为可序列化数据 |
| `Deserialize` | 从 JSON 等数据构造 Rust 值 |
| `sqlx::FromRow` | 从数据库行读取结构体字段 |

`{}` 使用 `Display`，适合可读文本；`{:?}` 使用 `Debug`，适合检查结构。派生 `Debug` 不会自动获得 `Display`。

`#[serde(rename = "type")]` 控制 JSON 字段名；`#[sqlx(rename = "type")]` 控制数据库列名。本项目 Rust 字段叫 `channel_type`，JSON 和 SQL 中仍叫 `type`。

### 10. JSON、async 与 HTTP

`serde_json::Value` 能表示对象、数组、字符串、数字、布尔值或 null，因此“是 JSON”不代表“是对象”。

```rust
let mut body = serde_json::json!({"model": "demo"});
let object = body.as_object_mut().expect("这里明确创建了对象");
object.insert("stream".into(), serde_json::Value::Bool(true));
```

`async fn` 返回 Future，运行时负责推进执行；`.await` 等待异步操作完成，不等于自动创建新线程。项目由 Tauri 的异步运行时执行数据库与 HTTP 操作。

```rust
let response = client
    .post(url)
    .bearer_auth(api_key) // Authorization: Bearer ...
    .json(&body)         // 序列化 JSON，设置 Content-Type
    .send()
    .await?;

let status = response.status(); // 先读取状态
let text = response.text().await?; // 消费响应并读取完整 body
```

`send().await` 得到响应时，不代表整个响应体已经读完。`text()`、`json()` 会消费响应；流式处理应保留 Response，再调用 `chunk().await` 逐块读取。

`#[async_trait]` 帮助本项目通过 `dyn Adapter` 调用异步 trait 方法。`Adapter: Send + Sync` 要求实现可安全跨线程转移并共享引用；`State<'_, AppState>` 则表示本次 command 借用 Tauri 管理的共享状态。

## 项目概况与目录

bingeAPI 是 Tauri 桌面应用：React + TypeScript 构建界面，Rust 处理业务，SQLite 保存数据，reqwest 请求上游 LLM API。

| 功能 | 当前状态 |
| --- | --- |
| 渠道增删改查、计数、连通性测试 | 已注册为 Tauri command |
| OpenAI 兼容的普通转发、流式转发 | 已实现适配器方法，尚未接入代理服务入口 |
| 按启用状态查客户端密钥、搜索日志 | 已实现数据库函数，尚未注册 command |
| 渠道管理页面 | 尚未实现，前端仍是 greet 示例 |
| 客户端鉴权、额度/过期检查、选路、重试、请求日志写入 | 尚未实现完整业务流程 |
| SSE 内容解析、流式用量统计、安全审计 | 尚未实现 |

### 目录与职责

```text
bingeAPI/
├── LLMAPI.md                       项目主文档
├── package.json / bun.lock         前端脚本与依赖
├── vite.config.ts / tsconfig*.json 前端构建与类型检查
├── src/
│   ├── main.tsx                    React 挂载入口
│   ├── App.tsx                     当前示例页面，调用 greet
│   └── App.css                     页面样式
└── src-tauri/
    ├── Cargo.toml / Cargo.lock     Rust 依赖
    ├── tauri.conf.json             窗口、应用标识、开发与打包配置
    ├── capabilities/default.json  主窗口插件权限
    ├── build.rs                    Tauri 构建、监听迁移文件变化
    ├── migrations/0001_initial.sql 数据库初始版本
    └── src/
        ├── main.rs                 调用 bingeapi_lib::run()
        ├── lib.rs                  初始化、组装 AppState、注册 command
        ├── error.rs                AppError 与前端错误转换
        ├── models/channel.rs       渠道业务类型和输入输出
        ├── commands/channel.rs     前端 IPC 入口
        ├── services/
        │   ├── channel.rs          校验、渠道测试、保存测试结果
        │   └── channel/tests.rs    业务测试
        ├── db/
        │   ├── mod.rs              数据库连接、迁移、时间函数
        │   ├── channel.rs          渠道 SQL 与 JSON 存储转换
        │   ├── api_key.rs          启用密钥查询
        │   ├── request_log.rs      日志查询、筛选与分页
        │   ├── tests.rs            数据库测试
        │   └── fixtures/legacy.sql 旧版表结构，仅供迁移测试
        └── adapter/
            ├── mod.rs              Adapter 接口、共享类型、工厂
            ├── presets.rs          供应商配置与测试策略
            ├── openai_compatible.rs 公共 Chat Completions 实现
            └── tests.rs            请求构造、HTTP、SSE 测试
```

Rust 子目录中的 `mod.rs` 负责声明或导出模块。旧目录 `adaptor/` 已统一为 `adapter/`；原 `db/repository.rs` 按业务拆分，原根目录 `init.sql` 已迁入 `migrations/`。

| 模块 | 应该负责 | 不应承担 |
| --- | --- | --- |
| `lib.rs` | 创建资源、注册入口 | 具体业务规则 |
| `commands` | 接收参数、调用函数、返回错误 | SQL、HTTP 协议细节 |
| `services` | 校验输入、协调多个步骤 | Tauri 窗口和状态管理 |
| `db` | SQL、存储转换、数据库迁移 | 请求上游 |
| `adapter` | 上游认证、请求和响应协议 | 查询数据库 |
| `models` | 表达业务数据与接口结构 | 依赖 Tauri、SQLx |

`repository` 是“数据访问层”的称呼；当前代码里的 `use crate::db::channel as repository` 只是给渠道数据库模块取别名。

### 渠道为什么有几种类型

| 类型 | 所在位置 | 用途 |
| --- | --- | --- |
| `CreateChannelInput` / `UpdateChannelInput` | `models/channel.rs` | 接收创建和部分更新参数 |
| `ChannelRow` | `db/channel.rs`，私有 | 对应 SQLite 行，JSON 字段是 String |
| `Channel` | `models/channel.rs` | 后端业务数据，包含 `info` 和真实 `api_key` |
| `ChannelInfo` | `models/channel.rs` | 返回前端，结构化 JSON + `has_api_key` |
| `ChannelConfig` | `adapter/mod.rs` | 适配器所需的地址、密钥、模型和配置 |

```text
SQLite 行 → ChannelRow → Channel
                          ├─ info → ChannelInfo → 前端
                          └─ 连接相关字段 → ChannelConfig → 上游
```

JSON 文本在数据库边界解析一次。command 不需要自己调用 `serde_json::from_str`；适配器也不需要认识数据库字段。

## 程序逻辑与调用流程

### 1. 应用启动

代码入口：[lib.rs](src-tauri/src/lib.rs)。

```text
main.rs → lib.rs::run()
            ↓
注册 opener 插件
            ↓
取得应用数据目录并创建目录
            ↓
Database::open(path)：创建连接池、执行数据库迁移
            ↓
创建共享 reqwest::Client
            ↓
app.manage(AppState { database, http_client })
            ↓
注册 command，运行桌面应用
```

数据库和 Client 都在启动时创建。command 通过 `tauri::State<'_, AppState>` 借用这些资源，避免每次打开数据库、重新创建 HTTP 连接池。

数据库初始化是异步的，Tauri 的 `setup` 闭包是同步的，所以入口用 `tauri::async_runtime::block_on` 等初始化完成。数据库不可用时会中止启动。

### 2. 前端调用与查询

```text
React invoke("list_channels")
    → commands::list_channels
    → db::channel::list_channels
    → SQL 查询、ChannelRow 转换
    → 提取 ChannelInfo
    → 序列化后返回 React
```

`invoke` 是前端与 Rust 之间的 IPC 调用，不是请求某个 HTTP 路由。函数需要有 `#[tauri::command]`，并在 `lib.rs` 的 `generate_handler!` 中注册。

计数、查询、删除直接调用数据库函数；创建、更新和测试需要业务处理，经过 service。简单查询不增加只有转发作用的中间层。

### 3. 创建与部分更新

创建：前端参数 → service 校验 → 后端生成 UUID/时间 → JSON 序列化 → INSERT → 查回完整渠道 → 返回 ChannelInfo。

更新：前端参数 → 校验已传字段 → UPDATE → 查回渠道 → 返回 ChannelInfo。

数据库使用：

```sql
name = COALESCE(?, name)
```

参数不是 SQL NULL 就写入新值，否则保留旧列。这样可以只改名称，不必把完整记录重新传一遍。更新时会刷新 `updated_at`；不存在的 ID 返回错误。

### 4. 渠道连通性测试

实现：[services/channel.rs](src-tauri/src/services/channel.rs)。

```text
test_channel(id)
    → 查渠道，解析存储配置
    → 按渠道类型选择 Adapter
    → 构造 ChannelConfig
    → adapter.test(config)
    → 更新 last_test_at / last_test_ok
    → 返回 TestResult 或内部错误
```

结果分为三类：

| 情况 | 返回 | 最近测试状态 |
| --- | --- | --- |
| 上游返回成功状态 | `success: true` | 记录为 1 |
| 无法连接或上游拒绝 | `success: false`，附带原因 | 记录为 0 |
| 调用 `adapter.test` 返回内部错误 | `Err` | 尝试记录为 0 后返回错误 |

查不到渠道、存储 JSON 无法解析或类型不受支持时，在调用适配器之前就返回错误；这些情况不会写入测试结果。保存结果本身失败时，command 返回数据库错误。

当前 `latency_ms` 主要测量到收到响应头的耗时，不代表完整回答生成时间；OpenAI 的测试成功也不代表所有聊天模型都可用。

### 5. 完整代理流程的后续方向

以下是待接入流程，当前还不能直接对外提供完整代理服务：

```text
客户端携带本项目的 API Key
    → 鉴权、检查模型权限/额度/有效期
    → 选择渠道
    → 调用 adapter.forward 或 forward_stream
    → 将响应返回客户端
    → 写 request_logs、更新额度
```

当前已有“查启用密钥”“上游转发”“查日志”等部件，尚未完成这条调用链。数据库有某个字段，不等于已经实现了对应规则。

## 前后端接口

实现：[commands/channel.rs](src-tauri/src/commands/channel.rs)。

### command 清单

| command | invoke 参数 | 成功返回 |
| --- | --- | --- |
| `greet` | `{ name }` | 问候字符串 |
| `count_channels` | 无 | 全部渠道数量 |
| `list_channels` | 无 | `ChannelInfo[]`，按 priority 降序、created_at 降序 |
| `get_channel` | `{ id }` | `ChannelInfo`，不存在时为 null |
| `create_channel` | `{ input }` | 新建的 `ChannelInfo` |
| `update_channel` | `{ input }` | 更新后的 `ChannelInfo` |
| `delete_channel` | `{ id }` | true；不存在时报错 |
| `test_channel` | `{ id }` | `{ success, message, latency_ms }` |

列表包含启用和禁用渠道；当前手动测试也不会因为 `status = 0` 自动跳过。

### 创建和更新参数

创建必填：`name`、`type`、`base_url`、`api_key`、`models`。

创建可选：`priority` 默认 0，`weight` 默认 1，`config`、`model_mapping` 默认空对象。`status` 由后端固定初始化为 1。

更新必填 `id`；其余可更新字段都是可选的，另支持 `status`。

| 规则 | 当前行为 |
| --- | --- |
| name、api_key | 不允许空字符串或全空白 |
| type | 只支持 `openai`、`deepseek` |
| base_url | 必须是带主机的绝对 HTTP(S) URL，填写 API 根地址 |
| models | 字符串数组，可以为空；已有元素不能是空白 |
| model_mapping | 字符串到字符串的对象 |
| config | JSON 对象，内部值可为任意 JSON |
| weight | 正整数 |
| status | 更新时只接受 0 或 1 |
| priority | 整数，当前没有范围限制 |

更新语义：

| 输入 | 含义 |
| --- | --- |
| 字段省略或传 null | 保留旧值 |
| `models: []` | 清空模型列表 |
| `config: {}` / `model_mapping: {}` | 清空对应配置 |
| 传入部分 config 对象 | 替换整个对象，不会与旧对象合并 |
| 省略 api_key | 保留原密钥；不能用空字符串清空 |

### 调用示例

在 Tauri 窗口中执行；浏览器单独打开 Vite 页面不具备完整的 Tauri 调用环境。

```ts
import { invoke } from "@tauri-apps/api/core";

const channel = await invoke<{ id: string }>("create_channel", {
  input: {
    name: "My channel",
    type: "openai",
    base_url: "https://api.openai.com/v1",
    api_key: "YOUR_API_KEY", // 示例占位，创建时替换为实际值
    models: ["gpt-4o"],
    model_mapping: {},
    config: {},
  },
});

await invoke("update_channel", {
  input: { id: channel.id, name: "Renamed" },
});

const result = await invoke<{
  success: boolean;
  message: string;
  latency_ms: number;
}>("test_channel", { id: channel.id });
```

渠道返回值节选：

```json
{
  "id": "<uuid>",
  "name": "Renamed",
  "type": "openai",
  "has_api_key": true,
  "models": ["gpt-4o"],
  "config": {},
  "model_mapping": {},
  "last_test_ok": null
}
```

完整返回还包含地址、状态、优先级、权重和时间字段，详见 [ChannelInfo](src-tauri/src/models/channel.rs)。JSON 字段已经是数组或对象，不要再对它们调用 `JSON.parse`；返回值不含真实 `api_key`。

### 错误返回

内部保留 `AppError`，到 command 边界转换为：

```json
{ "code": "not_found", "message": "channel not found: missing-id" }
```

| code | 原因 |
| --- | --- |
| `invalid_input` | 业务校验失败 |
| `not_found` | 要操作的渠道不存在 |
| `database_error` | SQL 执行、连接或读取失败 |
| `migration_error` | 迁移失败，当前主要发生在启动时 |
| `invalid_stored_data` | 数据库中某个 JSON 字段无法解析 |
| `serialization_error` | JSON 序列化失败 |
| `upstream_error` | 适配器内部或转发过程错误 |

`invoke` 遇到 command 的 `Err` 会拒绝 Promise，应使用 `try/catch`。Tauri 自身的参数反序列化错误由框架处理，不能假设所有错误都有 `code`。

## 适配器与流式响应

实现：[adapter/mod.rs](src-tauri/src/adapter/mod.rs)、[openai_compatible.rs](src-tauri/src/adapter/openai_compatible.rs)。

### 公共协议与供应商预设

`get_adapter(type, &client)` 从 [presets.rs](src-tauri/src/adapter/presets.rs) 选预设，返回 `Box<dyn Adapter>`。当前两种渠道共用 URL 拼接、Bearer 认证、请求体构造、响应读取和用量解析。

| 预设 | 项目保存的默认地址/模型 | 连通性测试 | 特殊处理 |
| --- | --- | --- | --- |
| openai | `https://api.openai.com/v1`；gpt-4o、gpt-4o-mini | GET /models | config.organization / project 转成相应请求头 |
| deepseek | `https://api.edgefn.net/v1`；DeepSeekV4Pro | POST /chat/completions，max_tokens=1 | 默认别名映射到 DeepSeek-V4-Pro-0813 |

这些是项目内置数据，不表示官方或实时模型清单。DeepSeek 地址是项目已有的第三方 edgefn 预设；实际请求始终使用数据库保存的 `base_url`。创建渠道仍需自己传地址和模型；当前没有自动填充预设的前端页面。

模型映射顺序：用户 `model_mapping` → 供应商默认映射 → 原始模型名。DeepSeek 测试使用配置中的第一个模型；列表为空时使用预设模型。

HTTP Client 在启动时创建，各适配器 clone 共享连接池。配置了 10 秒连接超时；未设置覆盖整个响应的总超时，因此不等于“请求最多运行 10 秒”。

### 普通转发与流式转发

| 方法 | 请求体处理 | 返回 |
| --- | --- | --- |
| `forward` | 映射 model，覆盖 `stream = false` | `ProxyResponse { status, body, usage }` |
| `forward_stream` | 映射 model，覆盖 `stream = true` | 未消费的 `reqwest::Response` |

`ProxyRequest` 只有 `model`、`body`；流式开关由调用的方法决定。body 必须为 JSON 对象，其余字段透传。

普通转发遇到上游 4xx/5xx，仍返回状态码和响应数据，由调用方决定如何处理。非 JSON 响应保存为 `Value::String`。usage 缺失或所需字段不完整时返回 None，不能当作消耗为零。

### SSE 需要理解的几个边界

SSE 在一个持续打开的 HTTP 响应里发送事件，响应类型通常为 `text/event-stream`。Chat Completions 的流式调用仍是 POST 请求。

下面是简化的协议示例，不是新的实测结果：

```text
data: {"choices":[{"delta":{"reasoning_content":"先分析"}}]}

data: {"choices":[{"delta":{"content":"1 2 3"}}]}

data: {"choices":[{"delta":{},"finish_reason":"stop"}]}

data: {"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}

data: [DONE]

```

| 字段/标志 | 含义与处理 |
| --- | --- |
| `delta.content` | 本次新增的回答文本，按顺序拼接 |
| `delta.reasoning_content` | 某些上游的推理文本增量，和回答分别处理 |
| `finish_reason` | 生成结束原因；仍可能有后续用量事件 |
| `usage` | 用量快照；同一回答重复出现的累计用量不要反复相加 |
| `data: [DONE]` | OpenAI 兼容协议的结束标志，不是 JSON，也不是 SSE 标准的特殊值 |

不同上游的字段、事件顺序和 usage 行为可能不同；不能假设每个事件都有 `choices[0]` 或 content。

网络 chunk 不等于 SSE 事件：一个 chunk 可能包含多条事件，一条事件也可能跨多个 chunk，甚至一个 UTF-8 字符的字节也可能被拆开。

如果只转发，直接转发原始字节。如果需要解析，应按以下顺序处理：

```text
检查状态码和 Content-Type
    → 将字节加入缓冲区
    → 按 SSE 行规则组装完整事件（空行结束事件）
    → 合并多行 data，忽略注释/心跳
    → 先识别 [DONE]，再解析普通 JSON
    → 累计回答/推理内容，保存最新用量
```

实现解析器时要处理 LF、CRLF 等合法行结束形式，不要只对单个网络 chunk 做字符串替换和 JSON 解析。对于约定使用 `[DONE]` 的上游，提前断流需要作为不完整响应处理。

标准 SSE 还可以有 `event`（类型）、`id`（重连标识）、`retry`（重连间隔）和以 `:` 开头的注释行。原生 EventSource 不适合直接发送这里的 POST 请求；前端可用 fetch 读流，后端可用 reqwest。

当前代码只保留并读取/转发原始响应，没有生产用的 SSE 事件解析器。真实测试中的 `response.chunk().await` 是把收到的字节写到终端，不会自动生成完整回答或统计流式用量。

## 数据库结构

结构来源：[0001_initial.sql](src-tauri/migrations/0001_initial.sql)。目前有三张业务表，另有 SQLx 自动管理的 `_sqlx_migrations`。

### 存放位置与存储约定

数据库文件名为 `bingeapi.db`，放在 Tauri 的应用数据目录，不在项目源码目录中。当前应用标识为 `com.da.bingeapi`；macOS 通常对应：

```text
~/Library/Application Support/com.da.bingeapi/bingeapi.db
```

实际路径以 `app.path().app_data_dir()` 的结果为准。连接池最多保留 5 条连接，按需复用；它不是每次查询都新建的单条连接。

| 约定 | 当前实现 |
| --- | --- |
| ID | TEXT；创建渠道时由 Rust 生成 UUID v4 |
| 时间 | 业务表用 TEXT；渠道写入 UTC ISO 8601，精确到毫秒 |
| 布尔状态 | INTEGER，通常 0/1；Rust 部分字段仍用 i64 |
| JSON | SQLite TEXT；写入前序列化，渠道读取时解析 |
| 密钥 | 当前直接存为 TEXT；接口不返回渠道密钥，不等于数据库已经加密 |
| 关联 | 使用 ID 字段表达，没有外键约束 |
| 校验 | 主要在 service；表中没有 JSON、状态枚举或正权重的 CHECK 约束 |

例如渠道时间：`2026-09-05T12:34:56.789Z`。统一格式后可按文本比较时间；混用时区或格式会影响筛选和排序。

### channels：有哪些上游渠道

| 字段 | 类型 | 约束/默认值 | 含义 |
| --- | --- | --- | --- |
| `id` | TEXT | PRIMARY KEY | 渠道 ID，应用创建时生成 UUID |
| `name` | TEXT | NOT NULL | 展示名称 |
| `type` | TEXT | NOT NULL | 适配器类型，当前 openai / deepseek |
| `base_url` | TEXT | NOT NULL | API 根地址，如 https://example.com/v1 |
| `api_key` | TEXT | NOT NULL | 调用上游供应商的密钥 |
| `models` | TEXT | NOT NULL，默认 '[]' | 模型名 JSON 数组 |
| `status` | INTEGER | NOT NULL，默认 1 | 1 启用、0 禁用 |
| `priority` | INTEGER | NOT NULL，默认 0 | 优先级；当前用于列表排序 |
| `weight` | INTEGER | NOT NULL，默认 1 | 预留调度权重，尚未用于负载均衡 |
| `config` | TEXT | NOT NULL，默认 '{}' | 供应商扩展配置 JSON 对象 |
| `model_mapping` | TEXT | NOT NULL，默认 '{}' | 对外模型名到上游模型名的 JSON 映射 |
| `created_at` | TEXT | NOT NULL | 创建时间 |
| `updated_at` | TEXT | NOT NULL | 最近编辑时间 |
| `last_test_at` | TEXT | 可空 | 最近一次测试时间；未测试为 NULL |
| `last_test_ok` | INTEGER | 可空 | 测试成功 1、失败 0、未测试 NULL |

数据库中的 `models` 内容是 JSON 文本，例如 `["alias"]`；业务代码中是 `Vec<String>`。映射示例：

```json
{
  "model_mapping": {
    "alias": "actual-upstream-model"
  },
  "config": {
    "organization": "org-id",
    "project": "project-id"
  }
}
```

配置里的 organization、project 目前仅由 OpenAI 预设转换为请求头。

普通渠道编辑会更新 `updated_at`；保存测试结果只更新 `last_test_at` 和 `last_test_ok`，不会自动改变渠道启用状态或 `updated_at`。

### api_keys：哪些客户端可以调用本项目

这里的密钥是“客户端 → 本项目”的凭证，与 `channels.api_key` 的“本项目 → 上游供应商”凭证不同。

| 字段 | 类型 | 约束/默认值 | 含义 |
| --- | --- | --- | --- |
| `id` | TEXT | PRIMARY KEY | 记录 ID，用于关联 |
| `name` | TEXT | NOT NULL | 给人看的密钥名称 |
| `key` | TEXT | NOT NULL、UNIQUE | 客户端实际提交的密钥 |
| `status` | INTEGER | NOT NULL，默认 1 | 当前查询只匹配值为 1 的记录 |
| `allowed_models` | TEXT | NOT NULL，默认 '[]' | 预留允许模型的 JSON 数组 |
| `allowed_channels` | TEXT | NOT NULL，默认 '[]' | 预留允许渠道的 JSON 数组 |
| `quota_limit` | INTEGER | NOT NULL，默认 -1 | 预留总额度；-1 的业务语义尚未实现 |
| `quota_used` | INTEGER | NOT NULL，默认 0 | 预留已用额度 |
| `expires_at` | TEXT | 可空 | 预留过期时间 |
| `created_at` | TEXT | NOT NULL | 创建时间 |
| `updated_at` | TEXT | NOT NULL | 更新时间 |

[get_api_key_by_key](src-tauri/src/db/api_key.rs) 目前仅执行“密钥相同且 status = 1”的查询，返回 `Option<ApiKey>`。它不会检查额度、过期时间或允许范围；空数组究竟表示“不限制”还是“不允许”，也尚未实现规则。

目前 `ApiKey` 的 allowed_* 字段仍是数据库 JSON 字符串，没有像渠道一样转换成业务集合。创建密钥、扣减额度和前端管理接口尚未实现。

### request_logs：每次请求发生了什么

| 字段 | 类型 | 约束/默认值 | 含义 |
| --- | --- | --- | --- |
| `id` | TEXT | PRIMARY KEY | 日志 ID |
| `api_key_id` | TEXT | 可空 | 使用的客户端密钥记录 ID |
| `api_key_name` | TEXT | 可空 | 请求发生时的密钥名称快照 |
| `channel_id` | TEXT | 可空 | 实际使用的渠道 ID |
| `channel_name` | TEXT | 可空 | 请求发生时的渠道名称快照 |
| `model` | TEXT | NOT NULL | 客户端请求的模型名 |
| `upstream_model` | TEXT | 可空 | 映射后发送给上游的模型名 |
| `mode` | TEXT | NOT NULL | 请求模式，取值规则尚未统一 |
| `status_code` | INTEGER | NOT NULL | 响应状态码 |
| `prompt_tokens` | INTEGER | NOT NULL，默认 0 | 输入 Token 数 |
| `completion_tokens` | INTEGER | NOT NULL，默认 0 | 输出 Token 数 |
| `total_tokens` | INTEGER | NOT NULL，默认 0 | 总 Token 数 |
| `duration_ms` | INTEGER | NOT NULL，默认 0 | 请求耗时，毫秒 |
| `error_message` | TEXT | 可空 | 错误说明 |
| `is_stream` | INTEGER | NOT NULL，默认 0 | 是否流式请求 |
| `is_retry` | INTEGER | NOT NULL，默认 0 | 是否重试 |
| `created_at` | TEXT | NOT NULL | 日志产生时间 |

当前只实现查询，没有自动写入生产请求日志。表中也没有 `seq`、`request_body`、`risk_level` 等审计字段；Rust 模型已经与实际表结构对齐。

表中的 Token 默认值 0 是存储默认值，不能用来推断上游是否报告过用量；将来接入日志写入时，需要定义“用量未知”的处理方式。

### 表之间如何关联

```text
api_keys.id ──── request_logs.api_key_id
channels.id ──── request_logs.channel_id
```

一个密钥或渠道可以对应多条日志。这些是逻辑关联，不是数据库外键：删除渠道不会级联删除日志，也不会自动清空日志里的 channel_id。

名称快照用于保留请求发生时的名称；将来渠道或密钥改名，不需要改写历史日志。当前日志查询直接读取快照，没有 JOIN 渠道表。

### 索引与日志查询

| 索引 | 字段 | 对应用途 |
| --- | --- | --- |
| `idx_logs_created` | created_at | 时间筛选和排序 |
| `idx_logs_channel` | channel_id | 按渠道 ID 查询 |
| `idx_logs_api_key` | api_key_id | 按客户端密钥 ID 查询 |
| `idx_logs_model` | model | 按模型查询 |

主键和 `api_keys.key` 的唯一约束也由 SQLite 管理相应索引。当前日志函数按 channel_name 筛选，不是 channel_id；已有 ID 索引不等于会加速这个名称条件。

[search_logs](src-tauri/src/db/request_log.rs) 接收：

- `LogFilter`：keyword、channel_name、model、date_from、date_to，全部可选。
- `Pagination`：limit、offset；默认每页 50 条、偏移 0。
- 返回 `Vec<RequestLog>`，没有总条数或总页数。

keyword 模糊匹配密钥名称、渠道名称、模型、上游模型和错误消息；channel_name、model 为精确匹配。时间上下界分别使用 `>=` 和 `<=`。空白筛选值会被忽略。

结果按 created_at 降序排列，limit 被限制到 1–1000，offset 最小为 0。LIKE 参数通过绑定传入；其中的 `%`、`_` 仍有 SQL 通配符含义。

### SQLx 在这里怎么用

SQLx 负责异步执行、连接池、参数绑定和行映射。本项目使用 `query` / `query_as` 函数，SQL 主要在运行时检查；只有使用相应的 `query!` / `query_as!` 宏并配置检查环境，才有那类编译期 SQL 检查。

```rust
let row = sqlx::query_as::<_, ChannelRow>(&sql)
    .bind(id)
    .fetch_optional(pool)
    .await?;
```

| API | 用途 |
| --- | --- |
| `query(...).execute(...)` | INSERT / UPDATE / DELETE 等操作 |
| `query_as::<_, T>(...)` | 将结果行映射为实现 FromRow 的 T |
| `fetch_optional` | 零行得到 None，有行得到 Some |
| `fetch_one` | 获取一行，没有行时报错 |
| `fetch_all` | 获取所有匹配行 |
| `bind` / `push_bind` | 绑定参数，避免把用户输入拼进 SQL |
| `QueryBuilder` | 组合可选筛选条件 |
| `rows_affected` | 判断更新/删除影响了多少行 |

渠道查询显式列出字段。`format!` 只拼接代码里的固定列名；用户传入的 ID、名称等通过参数绑定进入 SQL。

读取渠道的 JSON 转换为：

```text
models        TEXT → Vec<String>
config        TEXT → Map<String, Value>
model_mapping TEXT → HashMap<String, String>
```

存量 JSON 非法或结构不匹配时，返回带字段名的 `invalid_stored_data`，不静默替换成空配置。

### 数据库迁移

启动时由 `sqlx::migrate!("./migrations")` 内嵌并执行版本迁移，`build.rs` 监听该目录变化。

| 场景 | 行为 |
| --- | --- |
| 新数据库 | 执行 0001，创建业务表和索引 |
| 旧版 init.sql 创建的数据库 | 通过 IF NOT EXISTS 保留已有表和数据，登记初始版本 |
| 重复启动 | 检查迁移记录，跳过已完成的版本 |
| 新增字段或索引 | 增加新的迁移文件，例如 `0002_add_log_field.sql` |

`IF NOT EXISTS` 只避免重复创建，不会自动修复已有表的缺失字段。当前兼容的是仓库旧版结构；任意手工改过的数据库需要另行迁移。

`_sqlx_migrations` 由 SQLx 管理，主要记录 version、description、installed_on、success、checksum 和 execution_time。它用于判断哪些版本已执行，以及迁移文件是否发生变化。

已经发布并执行的迁移文件应保持不变；升级通过新增文件完成。不要靠删除用户数据库实现升级。`db/fixtures/legacy.sql` 是测试基线，不参与正常启动。

## 开发与测试

### 日常命令

以下命令均在项目根目录运行。

| 目的 | 命令 |
| --- | --- |
| 安装前端依赖 | `bun install` |
| 只启动前端开发服务器 | `bun run dev` |
| 启动整个桌面应用 | `bun run tauri dev` |
| 前端类型检查和生产构建 | `bun run build` |
| Rust 编译检查 | `cargo check --manifest-path src-tauri/Cargo.toml` |
| Rust 静态检查 | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets` |
| 格式化 Rust | `cargo fmt --manifest-path src-tauri/Cargo.toml` |
| 只检查格式 | `cargo fmt --manifest-path src-tauri/Cargo.toml --check` |
| 运行后端测试 | `cargo test --manifest-path src-tauri/Cargo.toml --lib` |
| 构建桌面安装包 | `bun run tauri build` |
| 安装前端依赖 | `bun add 包名` |
| 安装前端开发依赖 | `bun add -D 包名` |
| 安装 Rust 依赖 | `cargo add --manifest-path src-tauri/Cargo.toml crate名` |

依赖已缓存时，可给 `cargo check/test/clippy` 加 `--offline`。`Cargo.lock`、`bun.lock` 锁定依赖解析结果；提交依赖变更时一并保留。

`bun run tauri dev` 会按 tauri.conf.json 启动 Vite、编译 Rust、打开桌面窗口并监听变化。开发地址当前为 `http://localhost:1420`；打包时使用前端构建目录 `dist/`。

### 测试分布

| 文件 | 覆盖内容 |
| --- | --- |
| [adapter/tests.rs](src-tauri/src/adapter/tests.rs) | 模型映射、请求头、测试策略、普通 HTTP、SSE |
| [services/channel/tests.rs](src-tauri/src/services/channel/tests.rs) | CRUD、部分更新、参数校验、错误类型、密钥隐藏、测试状态保存 |
| [db/tests.rs](src-tauri/src/db/tests.rs) | 旧数据库迁移及重复打开、日志筛选、分页与参数绑定 |

当前共 17 个测试，其中 16 个默认执行，1 个真实上游测试默认忽略。普通测试使用内存/临时 SQLite 和本机模拟 HTTP，不需要供应商密钥；HTTP 测试需要允许监听 `127.0.0.1`。

部分日志、密钥和转发方法尚未接入生产入口，因此普通编译可能出现未使用代码警告；不能把这些提示误认为功能已经接通。

### 手动运行真实 DeepSeek 流测试

在 zsh 中静默输入密钥：

```sh
read -s "DEEPSEEK_API_KEY?请输入 DeepSeek API Key: "
export DEEPSEEK_API_KEY
cargo test --manifest-path src-tauri/Cargo.toml streams_live_response -- --ignored --nocapture
unset DEEPSEEK_API_KEY
```

`read -s` 不回显输入，`export` 让 Cargo 和测试进程继承变量，Rust 用 `std::env::var` 读取；`unset` 清除当前 shell 中的变量。

可用 `DEEPSEEK_BASE_URL` 和 `DEEPSEEK_MODEL` 覆盖测试地址和模型。这个测试会真实联网并消耗额度，只检查上游成功、SSE 类型及收到 data，不验证回答语义。

### 修改功能时从哪里开始

| 要做的事 | 主要修改位置 |
| --- | --- |
| 增加渠道校验 | services/channel.rs，并补业务测试 |
| 增加渠道字段 | 新迁移 → db/channel.rs → models/channel.rs → service/command |
| 新增 OpenAI 兼容供应商 | adapter/presets.rs 的预设和选择分支，并补差异测试 |
| 接入不同协议 | 新 Adapter 实现，并修改 get_adapter 工厂 |
| 新增前端调用入口 | commands 中定义函数，在 lib.rs 注册 |
| 接入代理服务 | 新业务入口协调鉴权、选路、adapter 和日志写入 |
| 开发渠道页面 | 在 src 中按渠道功能组织组件、类型和 invoke 调用 |

新增功能时沿现有职责放置代码：协议变化集中在 adapter，表结构变化通过迁移，业务规则集中在 service。阅读源码可从 `lib.rs → commands/channel.rs → services/channel.rs → db/adapter` 开始。
