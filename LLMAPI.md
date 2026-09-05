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

### DeepSeek 流式返回格式与 SSE

本项目对 DeepSeek 发出的流式请求仍是普通 HTTP POST，只是在 JSON 请求体中加入：

```json
{
  "model": "DeepSeek-V4-Pro-0813",
  "messages": [
    {
      "role": "user",
      "content": "请依次输出1到5，每个数字之间用空格分隔。"
    }
  ],
  "stream": true
}
```

真实测试返回：

```text
Status: 200 OK
Content-Type: text/event-stream
```

- `200 OK`：地址、API Key、模型和请求格式均被上游接受。
- `text/event-stream`：响应体采用 SSE 流，而不是一次性 JSON。

#### 什么是 SSE

SSE 全称 Server-Sent Events，作用是在一个保持打开的 HTTP 响应中，让服务端不断向客户端发送事件。

```text
客户端发送一次 HTTP 请求
        ↓
服务端保持响应连接
        ↓
data: 第一条事件

data: 第二条事件

data: 第三条事件
        ↓
服务端发送结束标志并关闭响应
```

SSE 是服务端到客户端的单向数据流；WebSocket 则支持双方持续互发消息。LLM 生成内容只需要服务端持续推送，因此 SSE 很合适。

标准 SSE 事件可以包含：

```text
event: message
id: 123
retry: 3000
data: 第一行数据
data: 第二行数据

```

- `event`：事件类型，省略时通常按普通 `message` 处理。
- `id`：SSE 事件 ID，用于断线重连；它不等同于 JSON 中的请求 `id`。
- `retry`：建议客户端等待多少毫秒后重连。
- `data`：事件数据；可以有多行，多行数据用换行连接。
- 空行：表示当前事件结束。
- 以 `:` 开头的行：注释或心跳，不是业务数据。

DeepSeek 的 OpenAI 兼容接口在本次测试中只使用了 `data:`。

#### 推理内容事件

流开始后，模型先返回推理增量：

```text
data: {"id":"<request-id>","object":"chat.completion.chunk","created":1788546113,"model":"DeepSeek-V4-Pro-0813","choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning_content":"我们需要"}}]}

data: {"id":"<request-id>","object":"chat.completion.chunk","created":1788546113,"model":"DeepSeek-V4-Pro-0813","choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning_content":"回答"}}]}
```

主要字段：

- `id`：本次模型请求的 ID；同一次回答的各事件通常使用相同 ID。
- `object`：`chat.completion.chunk` 表示它是回答的一小段增量。
- `created`：上游生成的 Unix 时间戳。
- `model`：实际上游模型。
- `choices[0].index`：第几个候选回答，通常为 `0`。
- `delta`：本次事件新增的内容，不是截至当前的完整内容。
- `delta.reasoning_content`：推理内容增量。
- `delta.content`：展示给用户的最终回答增量。

推理阶段通常表现为：

```text
reasoning_content 有内容
content           为空字符串
```

如果产品只展示最终答案，就不要把 `reasoning_content` 拼进 `content`；需要展示推理时，应分别保存和展示。

#### 最终回答事件

推理完成后，`content` 开始逐步返回：

```text
data: {"id":"<request-id>","object":"chat.completion.chunk","created":1788546113,"model":"DeepSeek-V4-Pro-0813","choices":[{"index":0,"delta":{"role":"assistant","content":"1"}}]}

data: {"id":"<request-id>","object":"chat.completion.chunk","created":1788546113,"model":"DeepSeek-V4-Pro-0813","choices":[{"index":0,"delta":{"role":"assistant","content":" "}}]}

data: {"id":"<request-id>","object":"chat.completion.chunk","created":1788546113,"model":"DeepSeek-V4-Pro-0813","choices":[{"index":0,"delta":{"role":"assistant","content":"2"}}]}
```

客户端按事件顺序拼接所有非空 `delta.content`：

```text
"1" + " " + "2" + " " + "3" + " " + "4" + " " + "5"
                              ↓
                         "1 2 3 4 5"
```

不要假设一次事件一定返回一个完整汉字、单词或句子；一次增量可能只有一个字符，也可能包含多个字符。

#### 正常结束与 Token 用量

模型完成生成时，本次测试收到：

```json
{
  "id": "<request-id>",
  "object": "chat.completion.chunk",
  "created": 1788546113,
  "model": "DeepSeek-V4-Pro-0813",
  "choices": [
    {
      "index": 0,
      "delta": {
        "role": "assistant",
        "content": ""
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 97,
    "completion_tokens": 52,
    "total_tokens": 149,
    "completion_tokens_details": {
      "reasoning_tokens": 42
    }
  }
}
```

- `finish_reason: "stop"`：模型正常完成回答。
- `prompt_tokens`：输入消耗的 Token。
- `completion_tokens`：输出消耗的 Token，可能包含推理 Token。
- `reasoning_tokens`：推理部分消耗的 Token。
- `total_tokens`：总消耗，本次为 `97 + 52 = 149`。

本次上游还额外发送了一条只有 usage、没有 choices 的事件：

```json
{
  "choices": [],
  "usage": {
    "prompt_tokens": 97,
    "completion_tokens": 52,
    "total_tokens": 149
  }
}
```

因此记录用量时不要把多次出现的 usage 累加。可以用最新的非空 usage 覆盖旧值，最终保存一次。

最后一条数据是：

```text
data: [DONE]
```

`[DONE]` 不是 JSON，也不是 SSE 标准规定的特殊值，而是 OpenAI 兼容接口约定的流结束标志。收到后应停止解析本次回答并完成收尾。

#### 网络 chunk 不等于 SSE 事件

`reqwest` 的：

```rust
response.chunk().await
```

读取的是网络字节块，不是完整 SSE 事件。二者边界没有固定关系：

```text
一个网络 chunk 可能包含多条 SSE 事件
一条 SSE 事件也可能被拆进多个网络 chunk
一个 UTF-8 汉字的字节也可能跨越两个 chunk
```

因此不能假设每次 `chunk()` 都能直接去掉 `data:` 后解析 JSON。

- 只做代理转发：可以将收到的原始字节立即转发，不必解析。
- 需要解析内容：先把字节加入缓冲区，再按 SSE 空行边界拆出完整事件，最后解析 `data:`。
- 解析器应同时考虑 `\n\n` 和 `\r\n\r\n` 行结束格式。
- 不要对每个原始 chunk 单独做 UTF-8 解码；应等完整数据边界或使用增量 UTF-8 解码器。

本项目的真实测试使用：

```rust
while let Some(chunk) = response.chunk().await? {
    std::io::stdout().write_all(&chunk)?;
}
```

它只是把原始字节写到终端，所以适合观察响应；如果以后需要提取 `content`、`reasoning_content` 和 `usage`，还需要增加 SSE 缓冲与解析逻辑。

#### 接收 DeepSeek SSE 的处理顺序

```text
检查 HTTP 状态码
    ↓ 非 2xx：读取普通错误响应
确认 Content-Type 是 text/event-stream
    ↓
持续读取网络 chunk，并加入缓冲区
    ↓
从缓冲区拆出完整 SSE 事件
    ↓
忽略空行、注释和心跳
    ↓
data: [DONE] → 正常结束
    ↓ 其他 data
解析 JSON
    ├─ reasoning_content → 单独累计推理内容
    ├─ content → 累计最终回答
    ├─ finish_reason → 记录结束原因
    └─ usage → 覆盖保存最新用量
```

如果连接在收到 `[DONE]` 前意外断开，应视为不完整响应，不能仅凭已经收到部分文本就判断请求完整成功。

浏览器原生 `EventSource` 主要用于 GET；Chat Completions 使用 POST，所以通常使用 `fetch()` 读取响应流，或由 Rust 后端使用 `reqwest` 接收并转发。

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
