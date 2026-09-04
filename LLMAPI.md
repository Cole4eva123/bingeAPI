> 我的要求：精炼易懂。

# LLMAPI



```
rustup
 ├── rustc      Rust 编译器
 ├── cargo      Rust 包管理器 / 构建工具
 ├── rustfmt    格式化  //自动把你的 Rust 代码整理成官方推荐的统一格式。
 └── clippy     代码检查
```

## Rust语法

函数最后一个没有 `;` 的表达式，就是返回值。

### 数组

```rust
let blank1 = [0; 3]; //就是创建了一个有三个值的数组，每个值都是0
```



### 引用

  - T：拿走所有权
  - &T：只读借用
  - &mut T：可修改借用

### `&str` 与 `String`

字符串字面量看起来像字符串，但类型是 `&'static str`：

```rust
"example-key" // 类型：&'static str
```

- `&str`：借用的字符串切片，不拥有内容；字符串字面量在程序运行期间一直有效。
- `String`：拥有字符串内容，声明为 `mut` 后可以修改和增长。

如果结构体字段要求 `String`：

```rust
pub api_key: String,
```

赋值时需要把 `&str` 转成 `String`：

```rust
api_key: "example-key".to_string(),
```

```text
&str → String
```

下面三种写法效果相同：

```rust
"example-key".to_string()
String::from("example-key")
"example-key".into() // 根据目标类型推断为 String
```

记忆：`&str` 是借用，`String` 是拥有。

### `Some` 与 `Option<T>`

`Some` 表示“这里有一个值”。它不是单独使用的类型，而是 `Option<T>` 的一种状态。

Rust 中的 `Option<T>` 可以简化理解为：

```rust
enum Option<T> {
    Some(T), // 有值，值的类型是 T
    None,    // 没有值
}
```

例如一个端口号可能存在，也可能不存在：

```rust
let port1: Option<u16> = Some(8080); // 有端口号
let port2: Option<u16> = None;       // 没有端口号
```

可以把 `Option<T>` 想成一个盒子：

```text
Some(8080) = 盒子里装着 8080
None       = 空盒子
```

因此 `Some(8080)` 的类型不是 `u16`，而是 `Option<u16>`。

#### 为什么需要 `Option<T>`

Rust 没有普通的 `null`。当一个值可能不存在时，必须明确写成 `Option<T>`，调用者也必须处理“有值”和“没有值”两种情况。

```rust
fn find_name(id: u64) -> Option<String> {
    if id == 1 {
        Some("Alice".to_string())
    } else {
        None
    }
}
```

#### `Some(value)`：把值装进去

```rust
let age = Some(18);
```

这里的 `Some(18)` 是构造一个 `Option<i32>`，把 `18` 放进去。

#### `Some(value)`：把值取出来

使用 `match` 同时处理两种情况：

```rust
match find_name(1) {
    Some(name) => println!("找到：{name}"),
    None => println!("没有找到"),
}
```

这里的 `Some(name)` 是模式匹配：如果盒子里有值，就把值取出来并命名为 `name`。

只关心有值时，可以使用 `if let`：

```rust
if let Some(name) = find_name(1) {
    println!("找到：{name}");
}
```

如果结果是 `None`，大括号里的代码不会执行。

#### 项目代码逐层拆解

```rust
if let Some(body) = request.body().and_then(|body| body.as_bytes()) {
    println!("Body: {}", String::from_utf8_lossy(body));
}
```

第一步：取得请求体。

```rust
request.body() // Option<&reqwest::Body>
```

- `Some(&Body)`：请求中有 body。
- `None`：请求中没有 body。

第二步：尝试把 body 读取成内存中的字节。

```rust
body.as_bytes() // Option<&[u8]>
```

- `Some(&[u8])`：body 可以直接作为字节读取。
- `None`：例如 body 是无法直接读取的流式数据。

第三步：`and_then` 把两次判断串起来。

```rust
request.body().and_then(|body| body.as_bytes())
```

等价逻辑：

```rust
match request.body() {
    Some(body) => body.as_bytes(),
    None => None,
}
```

只有 `request.body()` 是 `Some` 时，闭包 `|body| body.as_bytes()` 才会执行。最终结果是 `Option<&[u8]>`。

第四步：`if let Some(body)` 取出字节。

```rust
if let Some(body) = 最终结果 {
    // body 的类型是 &[u8]
}
```

整句话就是：如果请求体存在，并且能直接读取成字节，就取出这些字节并打印；否则什么也不做。

#### 常用方法

```rust
value.unwrap()              // 取出值；None 时程序崩溃
value.expect("必须有值")    // 取出值；None 时带消息崩溃
value.unwrap_or(默认值)     // None 时使用默认值
value.map(|v| 处理v)        // 有值才处理，并重新装进 Some
value.and_then(|v| 返回Option) // 有值才继续下一次可能失败的操作
```

业务代码中不要随意使用 `unwrap()`；优先使用 `match`、`if let`、`unwrap_or` 或 `?`。

在返回值也是 `Option` 的函数中，`?` 表示“遇到 `None` 就立即返回 `None`”：

```rust
fn first_char(name: Option<&str>) -> Option<char> {
    let name = name?;
    name.chars().next()
}
```

#### `Option` 和 `Result` 的区别

```text
Option<T>    Some(value) / None       只表示有值或没值
Result<T, E> Ok(value)   / Err(error) 表示成功或失败，并能携带错误原因
```

一句话记忆：`Some(value)` 就是“这个可能不存在的值，现在确实存在”。

### |

最常见的是 **闭包参数** 和 **按位或**。

```rust
let add = |a, b| a + b;

println!("{}", add(1, 2));
```

### #[derive(Debug)] and Display

有了这个之后才可以

```rust
println!("{:?}", state);
```

{:?}
 ↓
Debug
 ↓
程序员看

{}
 ↓
Display
 ↓
用户看

### 方法

```rust
Struct::funcName(&实例) ≈ 实例.funcName()
```

### 使用特征作为函数参数

```rust
pub fn notify(item: &impl Summary) {
    println!("Breaking news! {}", item.summarize());
}
```

```rust
pub fn notify<T: Summary>(item: &T) {
    println!("Breaking news! {}", item.summarize());

```

### Box

`Box<T>`：把值 `T` 放到堆上，`Box` 保存它的地址并拥有它。

```rust
let number = Box::new(10);
```

```text
number（固定大小的指针）→ 堆上的 10
```

本项目使用的是 `Box<dyn Adaptor>`：

```rust
pub fn get_adaptor(channel_type: &str) -> anyhow::Result<Box<dyn Adaptor>> {
    match channel_type {
        "openai" => Ok(Box::new(OpenAiAdapter::new())),
        _ => Err(anyhow::anyhow!("不支持的渠道类型")),
    }
}
```

- `dyn Adaptor`：某个实现了 `Adaptor` 的对象，具体类型可以运行时再确定。
- `Box`：提供固定大小的指针，因此函数可以统一返回不同大小的适配器。
- 调用 `adaptor.forward(...)` 时，Rust 会自动找到内部真实类型的方法。

以后可以统一返回不同供应商：

```rust
"openai" => Box::new(OpenAiAdapter::new()),
"claude" => Box::new(ClaudeAdapter::new()),
```

一句话：`Box<dyn Adaptor>` 把不同的适配器装进同一种“盒子”，让上层只依赖统一的 `Adaptor` 接口。

###   pub body: serde_json::Value

  只保证 body 是合法 JSON，不保证它一定是这种对象：

  {
    "messages": [],
    "model": "..."
  }

### Rust 用 reqwest 发 HTTP 请求

项目使用 `reqwest::Client`：

```rust
let client = reqwest::Client::new();

let response = client
    .post(url)                    // 创建 POST 请求
    .bearer_auth(api_key)         // 添加 Authorization: Bearer ...
    .json(&body)                  // JSON 请求体；自动设置 Content-Type
    .send()                       // 真正发送请求
    .await?;                      // 异步等待响应
```

读取响应：

```rust
let status = response.status();
let body = response.text().await?;
```

GET 请求：

```rust
let response = client.get(url).send().await?;
```

常用方法：

- `.get()` / `.post()`：选择请求方式。
- `.header()`：添加请求头。
- `.bearer_auth()`：添加 Bearer API Key。
- `.json()`：序列化 JSON，并自动设置 `Content-Type: application/json`。
- `.send().await`：发送请求并等待响应。
- `.status()`：取得 HTTP 状态码。
- `.text().await` / `.json().await`：读取响应体；读取后会消费 `response`。

流式响应不要先调用 `.text()` 或 `.json()`，应保留 `Response`，再用 `.bytes_stream()` 逐块读取。

## Rust插件

### rust-analyzer

```
自动补全
类型提示
跳转定义
重构
错误检查
Cargo 项目分析
```

### Tauri

你用 Tauri 写桌面应用时，前端通常是 HTML/CSS/JS/TS，但浏览器本身不能随便访问系统功能，比如文件系统、剪贴板、通知、系统托盘、打开外部链接等。Tauri 插件就是把这些 **系统级能力安全地暴露给前端调用**。

### Even Better TOML

装上之后，VS Code 编辑 `.toml` 文件会更舒服，主要包括：

- **语法高亮**
- **自动补全**
- **格式化 TOML**
- **语法错误提示**
- 对一些配置提供类型/Schema 检查
- 鼠标悬停时显示部分字段信息



```
bun tauri dev
```

```
启动前端开发服务器
+
编译 Rust / Tauri 后端
+
打开桌面 App 窗口
+
监听代码变化
```

## 平时开发需要的命令

前端单独开发：

```
bun run dev
```

整个桌面程序：

```
bun tauri dev
```

安装 JS 包：

```
bun add 包名
```

安装开发依赖：

```
bun add -D 包名
```

安装 Rust crate：

```
cd src-tauri
cargo add crate名
```

检查 Rust：

```
cargo check
```

Lint：

```
cargo clippy
```

格式化：

```
cargo fmt
```

最后打包：

```
bun tauri build
```



## 数据库

### sqlx

Rust 生态中操作 SQLite 的库主要有两个：

- **rusqlite：同步 API，简单直接，但在 async 环境中会阻塞线程**
- **sqlx：异步 API，编译时检查 SQL，支持连接池，与 Tokio 深度集成**

我们使用 `sqlx` + `runtime-tokio`，配合 `SqlitePoolOptions` 建立连接池。

### 库表设计

```
用户拿 api_key 发请求
        ↓
api_keys 检查这个 key 能不能用
        ↓
根据模型、权限选择 channel
        ↓
channel 决定请求发到哪个上游 API
        ↓
调用 OpenAI / Anthropic / Gemini / 其他供应商
        ↓
把这次请求写进 request_logs
```

三张表负责的东西：

```
channels
→ “我有哪些上游 API 渠道？”

| 字段              | 含义                           |
| --------------- | ---------------------------- |
| `id`            | 渠道主键                         |
| `name`          | 渠道名称                         |
| `type`          | 渠道类型，例如 `openai`、`anthropic` |
| `base_url`      | 上游 API 地址                    |
| `api_key`       | 调用上游时使用的密钥                   |
| `models`        | 这个渠道支持哪些模型                   |
| `status`        | 渠道是否启用                       |
| `priority`      | 调度优先级                        |
| `weight`        | 同优先级情况下的负载均衡权重               |
| `config`        | 额外配置                         |
| `model_mapping` | 模型名称映射                       |
| `created_at`    | 创建时间                         |
| `updated_at`    | 更新时间                         |
| `last_test_at`  | 最近一次测试渠道时间                   |
| `last_test_ok`  | 最近测试是否成功                     |

api_keys
→ “哪些用户/客户端可以调用我的服务？”

| 字段                 | 含义         |
| ------------------ | ---------- |
| `id`               | 主键         | --> 它主要是给数据库内部关联使用的。
| `name`             | Key 名称     | --> 这是给人看的名字。
| `key`              | 真正 API Key |
| `status`           | 是否可使用      |
| `allowed_models`   | 允许调用哪些模型   |
| `allowed_channels` | 允许使用哪些渠道   |
| `quota_limit`      | 总额度        |
| `quota_used`       | 已使用额度      |
| `expires_at`       | 过期时间       |
| `created_at`       | 创建时间       |
| `updated_at`       | 更新时间       |


request_logs
→ “每次请求实际发生了什么？”

request_logs

id = 10001

api_key_id = 10
api_key_name = development-key

channel_id = 1 --＞　这次请求最后使用了哪个上游渠道
channel_name = OpenAI Official

model = gpt-5
upstream_model = gpt-5-2026-08

status_code = 200

prompt_tokens = 1000
completion_tokens = 500
total_tokens = 1500

duration_ms = 850

error_message = NULL

is_stream = false
is_retry = false
```

整个数据表中我们用text类型存储json
