//! DeepSeek 渠道适配器。
//!
//! DeepSeek 的 Chat Completions 接口兼容 OpenAI 请求格式。
//! `ChannelConfig.base_url` 应填写 API 根地址，例如
//! `https://api.edgefn.net/v1`。该适配器支持连通性测试、非流式请求，
//! 以及将未消费的 SSE 响应交给业务层继续转发。

use std::time::Instant;

use anyhow::Context;
use async_trait::async_trait;
use reqwest::{RequestBuilder, StatusCode};
use serde_json::Value;

use super::{Adaptor, ChannelConfig, ProxyRequest, TestResult, TokenUsage};

/// DeepSeek 渠道类型标识，对应数据库 `channels.type` 的值。
const CHANNEL_TYPE: &str = "deepseek";

/// DeepSeek（edgefn 代理）的默认 API 根地址。
const DEFAULT_BASE_URL: &str = "https://api.edgefn.net/v1";

/// 客户端统一使用的模型别名。
const DEFAULT_MODEL_ALIAS: &str = "DeepSeekV4Pro";

/// `DEFAULT_MODEL_ALIAS` 对应的上游真实模型名称。
const DEFAULT_UPSTREAM_MODEL: &str = "DeepSeek-V4-Pro-0813";

/// DeepSeek OpenAI 兼容接口的渠道适配器。
///
/// 内部持有一个 `reqwest::Client`，在多次请求之间复用底层连接池。
#[derive(Debug, Clone)]
pub struct DeepSeekAdapter {
    /// 可复用的 HTTP 客户端，避免每次请求都重新建立 TCP 连接。
    client: reqwest::Client,
}

impl DeepSeekAdapter {
    /// 创建一个新的 DeepSeek 适配器实例。
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// 将 API 根地址和接口路径安全拼接，避免出现重复的 `/`。
    fn endpoint(base_url: &str, path: &str) -> String {
        format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// 根据当前渠道的映射配置，得到真正发送给 DeepSeek 的模型名称。
    ///
    /// 解析优先级（从高到低）：
    /// 1. `config.model_mapping` 中的映射（用户在数据库中配置的）；
    /// 2. 本适配器内置的默认映射（`DeepSeekV4Pro` → `DeepSeek-V4-Pro-0813`）；
    /// 3. 原样透传客户端请求的模型名。
    fn resolve_model(config: &ChannelConfig, requested_model: &str) -> String {
        config
            .model_mapping
            .get(requested_model)
            .cloned()
            .unwrap_or_else(|| match requested_model {
                DEFAULT_MODEL_ALIAS => DEFAULT_UPSTREAM_MODEL.to_string(),
                _ => requested_model.to_string(),
            })
    }

    /// 将通用请求转换为 DeepSeek Chat Completions 请求体。
    ///
    /// `request.body` 保存客户端传入的动态 JSON，例如 `messages`、
    /// `temperature` 等字段。这里复制一份请求体，在副本中统一写入真正的模型名
    /// 和流式开关，不修改原始的 `ProxyRequest`。
    fn build_body(
        config: &ChannelConfig,
        request: &ProxyRequest,
        stream: bool,
    ) -> anyhow::Result<Value> {
        // request 是借用，不能直接取走 request.body 的所有权；clone() 会创建一份
        // 独立的 serde_json::Value。声明为 mut 是因为下面要修改其中的字段。
        let mut body = request.body.clone();

        // JSON 请求体必须是对象。
        let object = body
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("DeepSeek 请求的 body 必须是 JSON 对象"))?;

        // 向 JSON 对象写入经过映射转换的 model 字段。
        object.insert(
            "model".to_string(),
            Value::String(Self::resolve_model(config, &request.model)),
        );

        // 调用 forward() 时写入 false，调用 forward_stream() 时写入 true。
        // 由适配器强制指定，避免 request.body 中原有值与实际调用方式冲突。
        object.insert("stream".to_string(), Value::Bool(stream));

        Ok(body)
    }

    /// 从上游错误响应中优先提取 `error.message`，否则保留原始响应内容。
    ///
    /// DeepSeek 的错误格式与 OpenAI 兼容：`{ "error": { "message": "..." } }`。
    fn upstream_error(status: StatusCode, body: &str) -> String {
        let message = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| body.to_string());

        format!("DeepSeek 上游返回 HTTP {}: {}", status.as_u16(), message)
    }

    /// 从 DeepSeek 响应的 `usage` 对象提取统一 Token 用量。
    fn extract_usage(body: &Value) -> Option<TokenUsage> {
        let usage = body.get("usage")?;

        Some(TokenUsage {
            prompt_tokens: usage.get("prompt_tokens")?.as_u64()?,
            completion_tokens: usage.get("completion_tokens")?.as_u64()?,
            total_tokens: usage.get("total_tokens")?.as_u64()?,
        })
    }

    /// 创建带 Bearer 认证和 JSON 请求体的上游请求。
    ///
    /// `bearer_auth` 会生成 `Authorization: Bearer <API_KEY>`；`json` 会序列化
    /// 请求体并自动添加 `Content-Type: application/json`。
    fn chat_request(&self, config: &ChannelConfig, body: &Value) -> RequestBuilder {
        let url = Self::endpoint(&config.base_url, "chat/completions");

        self.client
            .post(url)
            .bearer_auth(&config.api_key)
            .json(body)
    }
}

/// 允许使用 `DeepSeekAdapter::default()` 创建默认实例，等同于 `DeepSeekAdapter::new()`。
impl Default for DeepSeekAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adaptor for DeepSeekAdapter {
    fn channel_type(&self) -> &'static str {
        CHANNEL_TYPE
    }

    fn default_models(&self) -> Vec<&'static str> {
        vec![DEFAULT_MODEL_ALIAS]
    }

    fn default_base_url(&self) -> &str {
        DEFAULT_BASE_URL
    }

    /// 通过发送最小聊天请求测试渠道连通性。
    ///
    /// edgefn 没有提供独立的模型列表接口，因此这里直接向
    /// `/chat/completions` 发送一个最多生成 1 Token 的最小请求，同时验证地址、
    /// API Key 和模型名是否都能正常使用。
    async fn test(&self, config: &ChannelConfig) -> Result<TestResult, anyhow::Error> {
        let started_at = Instant::now();

        let requested_model = config
            .models
            .first()
            .map(String::as_str)
            .unwrap_or(DEFAULT_MODEL_ALIAS);
        let body = serde_json::json!({
            "model": Self::resolve_model(config, requested_model),
            "messages": [{ "role": "user", "content": "Hi" }],
            "max_tokens": 1,
            "stream": false
        });

        let response = match self.chat_request(config, &body).send().await {
            Ok(response) => response,
            Err(error) => {
                return Ok(TestResult {
                    success: false,
                    message: format!("无法连接 DeepSeek 上游: {error}"),
                    latency_ms: started_at.elapsed().as_millis() as u64,
                });
            }
        };

        let status = response.status();
        let latency_ms = started_at.elapsed().as_millis() as u64;

        if status.is_success() {
            return Ok(TestResult {
                success: true,
                message: "DeepSeek 渠道连接成功".to_string(),
                latency_ms,
            });
        }

        let body = response.text().await.unwrap_or_default();
        Ok(TestResult {
            success: false,
            message: Self::upstream_error(status, &body),
            latency_ms,
        })
    }

    /// 执行非流式 Chat Completions 请求。
    async fn forward(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<(u16, Value, Option<TokenUsage>), anyhow::Error> {
        let body = Self::build_body(config, request, false)?;
        let response = self
            .chat_request(config, &body)
            .send()
            .await
            .context("请求 DeepSeek 上游失败")?;

        let status_code = response.status().as_u16();
        let response_body = response
            .text()
            .await
            .context("读取 DeepSeek 上游响应失败")?;
        let response_json = serde_json::from_str::<Value>(&response_body)
            .unwrap_or_else(|_| Value::String(response_body));
        let usage = Self::extract_usage(&response_json);

        Ok((status_code, response_json, usage))
    }

    /// 执行流式 Chat Completions 请求。
    ///
    /// 只等待上游返回响应头，不读取 body。调用方负责逐块读取并转发 SSE 事件。
    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error> {
        let body = Self::build_body(config, request, true)?;

        self.chat_request(config, &body)
            .send()
            .await
            .context("请求 DeepSeek 流式上游失败")
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;

    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
    use serde_json::json;

    use super::*;

    /// 创建测试用的 ChannelConfig。
    fn config(model_mapping: HashMap<String, String>) -> ChannelConfig {
        ChannelConfig {
            base_url: "https://api.edgefn.net/v1".to_string(),
            api_key: "test-key".to_string(),
            models: vec![DEFAULT_MODEL_ALIAS.to_string()],
            model_mapping,
            extra: json!({}),
        }
    }

    #[test]
    fn resolves_edgefn_model_alias() {
        let config = config(HashMap::new());

        assert_eq!(
            DeepSeekAdapter::resolve_model(&config, DEFAULT_MODEL_ALIAS),
            DEFAULT_UPSTREAM_MODEL
        );
    }

    #[test]
    fn builds_edgefn_chat_request() {
        // 创建一份测试配置；空 HashMap 表示不设置自定义模型映射。
        let config = config(HashMap::new());

        // 根据渠道配置和 JSON 请求体构造 HTTP 请求。
        // build() 只生成 Request 供测试检查，不会真正向上游发送请求。
        let request = DeepSeekAdapter::new()
            .chat_request(&config, &json!({ "model": DEFAULT_UPSTREAM_MODEL }))
            .build()
            // 如果 URL、请求头或请求体无法构造，立即让测试失败。
            .unwrap();

        println!("Method: {}", request.method());
        println!("URL: {}", request.url());
        println!("Headers: {:#?}", request.headers());

        // request.body() 返回 Option<&reqwest::Body>：
        // - Some(request_body)：这个 HTTP 请求带有请求体；
        // - None：这个 HTTP 请求没有请求体。
        //
        // and_then(...) 只会在 request.body() 为 Some 时运行闭包。
        // request_body.as_bytes() 再尝试把请求体转换为 &[u8]：
        // - 普通 JSON 请求体可以转换，返回 Some(&[u8])；
        // - 流式请求体等不能直接读取的类型会返回 None。
        //
        // if let Some(body_bytes) 会在最终得到字节时取出它们；
        // 任意一步返回 None，都不会进入下面的大括号。
        if let Some(body_bytes) = request
            .body()
            .and_then(|request_body| request_body.as_bytes())
        {
            // body_bytes 的类型是 &[u8]。from_utf8_lossy 将它转成可打印文本；
            // 遇到无效 UTF-8 字节时使用替代字符，而不会导致测试崩溃。
            println!("Body: {}", String::from_utf8_lossy(body_bytes));
        }

        // 验证 base_url 和接口路径被正确拼接。
        assert_eq!(
            request.url().as_str(),
            "https://api.edgefn.net/v1/chat/completions"
        );

        // bearer_auth("test-key") 应生成 Authorization: Bearer test-key。
        assert_eq!(request.headers()[AUTHORIZATION], "Bearer test-key");

        // 使用 .json(...) 后，reqwest 应自动设置 JSON Content-Type。
        assert_eq!(request.headers()[CONTENT_TYPE], "application/json");
    }

    #[test]
    fn builds_stream_request_body() {
        let request = ProxyRequest {
            model: DEFAULT_MODEL_ALIAS.to_string(),
            body: json!({
                "messages": [{ "role": "user", "content": "你好" }]
            }),
            stream: true,
        };

        let body = DeepSeekAdapter::build_body(&config(HashMap::new()), &request, true).unwrap();

        assert_eq!(body["model"], DEFAULT_UPSTREAM_MODEL);
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["content"], "你好");
    }

    /// 使用真实 API Key 访问 DeepSeek，并把收到的 SSE 字节逐块打印到终端。
    ///
    /// 默认被忽略，避免普通 `cargo test` 意外联网和消耗额度。运行方式：
    /// `cargo test streams_live_response -- --ignored --nocapture`
    #[test]
    #[ignore = "需要 DEEPSEEK_API_KEY，并会真实联网和消耗额度"]
    fn streams_live_response() {
        tauri::async_runtime::block_on(async {
            // API Key 通过当前终端的环境变量传入，不写入源代码或数据库。
            //
            // 在 zsh 中执行：
            //   read -s "DEEPSEEK_API_KEY?请输入 DeepSeek API Key: "
            //   export DEEPSEEK_API_KEY
            //
            // read -s 的作用：
            // 1. 等待用户在终端输入 Key；
            // 2. -s 表示静默输入，屏幕不会显示 Key；
            // 3. 将输入暂存在当前 zsh 进程的 DEEPSEEK_API_KEY 变量中。
            //
            // export 的作用：允许 zsh 启动的子进程继承这个变量。
            // 执行 cargo test 后，进程关系是：
            //   zsh（保存 Key）
            //     └── cargo
            //           └── Rust 测试进程（继承 Key）
            //
            // std::env::var() 只是从测试进程的环境中读取变量，不负责让用户输入，
            // 也不会把 Key保存到文件。变量不存在时，expect 会终止测试并显示提示。
            let api_key =
                std::env::var("DEEPSEEK_API_KEY").expect("请先设置环境变量 DEEPSEEK_API_KEY");

            // Base URL 和模型也允许通过环境变量临时覆盖。
            // 如果没有设置，则分别使用适配器内置的地址和模型别名。
            let base_url =
                std::env::var("DEEPSEEK_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
            let model =
                std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| DEFAULT_MODEL_ALIAS.to_string());

            // 把环境变量中读取到的 Key 放进仅存在于内存中的 ChannelConfig。
            // 后续调用链为：
            // config.api_key
            //   → chat_request(...)
            //   → reqwest 的 bearer_auth(...)
            //   → HTTP 请求头 Authorization: Bearer <API_KEY>
            let config = ChannelConfig {
                base_url,
                api_key,
                models: vec![model.clone()],
                model_mapping: HashMap::new(),
                extra: json!({}),
            };
            let request = ProxyRequest {
                model,
                body: json!({
                    "messages": [{
                        "role": "user",
                        "content": "请依次输出1到5，每个数字之间用空格分隔。"
                    }],
                    "max_tokens": 100
                }),
                stream: true,
            };

            let adapter = DeepSeekAdapter::new();
            let mut response = adapter
                .forward_stream(&request, &config)
                .await
                .expect("请求 DeepSeek 流式上游失败");

            let status = response.status();
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_string();

            println!("Status: {status}");
            println!("Content-Type: {content_type}");

            if !status.is_success() {
                let error_body = response.text().await.unwrap_or_default();
                panic!("上游返回失败状态 {status}: {error_body}");
            }

            assert!(
                content_type.contains("text/event-stream"),
                "上游没有返回 SSE，实际 Content-Type: {content_type}"
            );

            let mut received = Vec::new();
            while let Some(chunk) = response.chunk().await.expect("读取 SSE 数据块失败") {
                // 直接写入 stdout，保留上游原始 SSE 格式，并立即刷新终端。
                std::io::stdout().write_all(&chunk).unwrap();
                std::io::stdout().flush().unwrap();
                received.extend_from_slice(&chunk);
            }
            println!();

            let received = String::from_utf8_lossy(&received);
            assert!(received.contains("data:"), "响应中没有 SSE data 事件");
        });
    }
}
