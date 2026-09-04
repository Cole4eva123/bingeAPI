//! OpenAI 兼容渠道适配器。
//!
//! `ChannelConfig.base_url` 应填写 API 根地址，例如
//! `https://api.openai.com/v1`。该适配器支持连通性测试、非流式请求，
//! 以及将未消费的 SSE 响应交给业务层继续转发。

use std::time::Instant;

use anyhow::Context;
use async_trait::async_trait;
use reqwest::{RequestBuilder, StatusCode};
use serde_json::Value;

use super::{Adaptor, ChannelConfig, ProxyRequest, TestResult, TokenUsage};

/// OpenAI `/v1` 渠道类型标识，对应数据库 `channels.type` 的值。
const CHANNEL_TYPE: &str = "openai";

/// OpenAI 官方 API 根地址，作为渠道创建时的默认值。
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// OpenAI 以及兼容 OpenAI Chat Completions 协议的渠道适配器。
///
/// 内部持有一个 `reqwest::Client`，在多次请求之间复用底层连接池。
#[derive(Debug, Clone)]
pub struct OpenAiAdapter {
    /// 可复用的 HTTP 客户端，避免每次请求都重新建立 TCP 连接。
    client: reqwest::Client,
}

impl OpenAiAdapter {
    /// 创建一个新的 OpenAI 适配器实例。
    ///
    /// `reqwest::Client::new()` 会初始化一个带有连接池的 HTTP 客户端，
    /// 后续所有请求共享同一个客户端以提升性能。
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// 将 API 根地址和接口路径安全拼接，避免出现重复的 `/`。
    ///
    /// 例如 `endpoint("https://api.openai.com/v1/", "/models")`
    /// 返回 `"https://api.openai.com/v1/models"`。
    fn endpoint(base_url: &str, path: &str) -> String {
        format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// 根据当前渠道的映射配置得到真正发送给上游的模型名称。
    ///
    /// 优先使用 `config.model_mapping` 中的映射；如果没有匹配的映射，
    /// 则直接透传客户端请求的原始模型名。
    fn resolve_model(config: &ChannelConfig, requested_model: &str) -> String {
        config
            .model_mapping
            .get(requested_model)
            .cloned()
            .unwrap_or_else(|| requested_model.to_string())
    }

    /// 将通用请求转换为 OpenAI Chat Completions 请求体。
    ///
    /// `stream` 由调用的方法强制指定，而不是直接相信 `request.stream`：
    /// `forward` 必须生成普通 JSON 请求，`forward_stream` 必须生成 SSE 请求。
    /// 这样可以避免调用的方法与请求体中的 `stream` 状态互相矛盾。
    fn build_body(
        config: &ChannelConfig,
        request: &ProxyRequest,
        stream: bool,
    ) -> anyhow::Result<Value> {
        // request 是借用，不能直接取走 request.body 的所有权；clone() 会创建一份
        // 独立的 serde_json::Value。声明为 mut 是因为下面要修改其中的字段。
        let mut body = request.body.clone();

        // JSON 请求体必须是对象。
        // as_object_mut() 尝试把 Value 转换为一个可修改的 JSON 对象引用。
        let object = body
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("OpenAI 请求的 body 必须是 JSON 对象"))?;

        // 写入经过映射转换的 model 字段。
        object.insert(
            "model".to_string(),
            Value::String(Self::resolve_model(config, &request.model)),
        );
        // 写入 stream 字段，确保与调用方法一致。
        object.insert("stream".to_string(), Value::Bool(stream));
        Ok(body)
    }

    /// 添加 OpenAI 官方接口支持的可选组织和项目请求头。
    ///
    /// 渠道的 `extra` 可以配置为：
    /// `{ "organization": "org_...", "project": "proj_..." }`。
    /// 只有配置了对应字段时才会添加请求头。
    fn with_optional_headers(builder: RequestBuilder, config: &ChannelConfig) -> RequestBuilder {
        let mut builder = builder;

        if let Some(organization) = config.extra.get("organization").and_then(Value::as_str) {
            builder = builder.header("OpenAI-Organization", organization);
        }

        if let Some(project) = config.extra.get("project").and_then(Value::as_str) {
            builder = builder.header("OpenAI-Project", project);
        }

        builder
    }

    /// 从上游错误响应中优先提取 `error.message`，否则保留原始响应内容。
    ///
    /// OpenAI 的错误响应格式通常为 `{ "error": { "message": "..." } }`。
    /// 如果响应不是 JSON 或缺少该字段，则回退到使用原始响应体文本。
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

        format!("OpenAI 上游返回 HTTP {}: {}", status.as_u16(), message)
    }

    /// 从 OpenAI Chat Completions 的 `usage` 对象提取统一 Token 用量。
    ///
    /// 有些错误响应或兼容渠道不会返回 `usage`，任何必需字段缺失时都返回
    /// `None`，避免为了统计信息而丢弃本来可以正常转发的上游响应。
    fn extract_usage(body: &Value) -> Option<TokenUsage> {
        let usage = body.get("usage")?;

        Some(TokenUsage {
            prompt_tokens: usage.get("prompt_tokens")?.as_u64()?,
            completion_tokens: usage.get("completion_tokens")?.as_u64()?,
            total_tokens: usage.get("total_tokens")?.as_u64()?,
        })
    }

    /// 创建带有认证头、可选组织/项目头和 JSON 请求体的上游请求。
    ///
    /// `reqwest::Client` 会被多个请求复用，以便复用底层连接池。
    fn chat_request(&self, config: &ChannelConfig, body: &Value) -> RequestBuilder {
        let url = Self::endpoint(&config.base_url, "chat/completions");
        let request = self
            .client
            .post(url)
            .bearer_auth(&config.api_key)
            .json(body);

        Self::with_optional_headers(request, config)
    }
}

/// 允许使用 `OpenAiAdapter::default()` 创建默认实例，等同于 `OpenAiAdapter::new()`。
impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Adaptor for OpenAiAdapter {
    fn channel_type(&self) -> &'static str {
        CHANNEL_TYPE
    }

    fn default_models(&self) -> Vec<&'static str> {
        vec!["gpt-4o", "gpt-4o-mini"]
    }

    fn default_base_url(&self) -> &str {
        DEFAULT_BASE_URL
    }

    /// 通过调用 `/models` 端点测试渠道连通性。
    ///
    /// OpenAI 的 `/models` 只需要有效的 API Key，不会产生 Token 消耗，
    /// 比发送聊天请求更适合用于连通性测试。
    async fn test(&self, config: &ChannelConfig) -> Result<TestResult, anyhow::Error> {
        let started_at = Instant::now();
        let url = Self::endpoint(&config.base_url, "models");
        let request = self.client.get(url).bearer_auth(&config.api_key);
        let request = Self::with_optional_headers(request, config);

        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                return Ok(TestResult {
                    success: false,
                    message: format!("无法连接 OpenAI 上游: {error}"),
                    latency_ms: started_at.elapsed().as_millis() as u64,
                });
            }
        };

        let status = response.status();
        let latency_ms = started_at.elapsed().as_millis() as u64;

        if status.is_success() {
            return Ok(TestResult {
                success: true,
                message: "OpenAI 渠道连接成功".to_string(),
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
    ///
    /// 强制将 `stream` 设为 `false`，读取完整响应体并提取 Token 用量。
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
            .context("请求 OpenAI 上游失败")?;

        // 在消费 response 之前先保存状态码，因为 `.text()` 会取得 response
        // 的所有权并读取完整响应体。
        let status_code = response.status().as_u16();
        let response_body = response.text().await.context("读取 OpenAI 上游响应失败")?;

        // OpenAI 正常响应与错误响应通常都是 JSON。兼容服务偶尔会返回纯文本，
        // 此时使用 JSON String 保存原文，仍然把真实状态码和响应内容交给调用方。
        let response_json = serde_json::from_str::<Value>(&response_body)
            .unwrap_or_else(|_| Value::String(response_body));
        let usage = Self::extract_usage(&response_json);

        Ok((status_code, response_json, usage))
    }

    /// 执行流式 Chat Completions 请求。
    ///
    /// 强制将 `stream` 设为 `true`，只等待上游返回响应头，不读取 body。
    /// 调用方可以使用 `bytes_stream()` 逐块读取并转发 SSE 事件。
    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error> {
        let body = Self::build_body(config, request, true)?;

        self.chat_request(config, &body)
            .send()
            .await
            .context("请求 OpenAI 流式上游失败")
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    /// 创建测试用的 ChannelConfig。
    fn config() -> ChannelConfig {
        ChannelConfig {
            base_url: "https://api.openai.com/v1/".to_string(),
            api_key: "test-key".to_string(),
            models: vec!["gpt-4o".to_string()],
            model_mapping: HashMap::from([("gpt-4o".to_string(), "upstream-gpt-4o".to_string())]),
            extra: json!({}),
        }
    }

    #[test]
    fn joins_endpoint_without_duplicate_slash() {
        assert_eq!(
            OpenAiAdapter::endpoint("https://api.openai.com/v1/", "/models"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn applies_model_mapping_to_request_body() {
        let request = ProxyRequest {
            model: "gpt-4o".to_string(),
            body: json!({
                "messages": [{ "role": "user", "content": "hello" }]
            }),
            stream: false,
        };

        let body = OpenAiAdapter::build_body(&config(), &request, false).unwrap();

        assert_eq!(body["model"], "upstream-gpt-4o");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn extracts_token_usage() {
        let response = json!({
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 8,
                "total_tokens": 20
            }
        });

        let usage = OpenAiAdapter::extract_usage(&response).unwrap();

        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.completion_tokens, 8);
        assert_eq!(usage.total_tokens, 20);
    }
}
