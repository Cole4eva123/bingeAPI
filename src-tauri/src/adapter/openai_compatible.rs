use std::time::Instant;

use anyhow::Context;
use async_trait::async_trait;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::{json, Value};

use super::presets::{ProviderPreset, TestStrategy};
use super::{Adapter, ChannelConfig, ProxyRequest, ProxyResponse, TestResult, TokenUsage};

pub(super) struct OpenAiCompatible {
    client: Client,
    preset: &'static ProviderPreset,
}

impl OpenAiCompatible {
    pub fn new(client: Client, preset: &'static ProviderPreset) -> Self {
        Self { client, preset }
    }

    pub(super) fn endpoint(base_url: &str, path: &str) -> String {
        format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    pub(super) fn resolve_model(&self, config: &ChannelConfig, model: &str) -> String {
        config.model_mapping.get(model).cloned().unwrap_or_else(|| {
            self.preset
                .model_mapping
                .iter()
                .find(|(alias, _)| *alias == model)
                .map_or(model, |(_, upstream)| *upstream)
                .to_owned()
        })
    }

    pub(super) fn build_body(
        &self,
        config: &ChannelConfig,
        request: &ProxyRequest,
        stream: bool,
    ) -> anyhow::Result<Value> {
        let mut body = request.body.clone();
        let object = body
            .as_object_mut()
            .context("请求的 body 必须是 JSON 对象")?;
        object.insert(
            "model".into(),
            Value::String(self.resolve_model(config, &request.model)),
        );
        // 流式模式仅由 forward / forward_stream 决定，覆盖客户端 body 的值。
        object.insert("stream".into(), Value::Bool(stream));
        Ok(body)
    }

    fn authorize(&self, mut request: RequestBuilder, config: &ChannelConfig) -> RequestBuilder {
        request = request.bearer_auth(&config.api_key);
        for (field, header) in self.preset.extra_headers {
            if let Some(value) = config.extra.get(*field).and_then(Value::as_str) {
                request = request.header(*header, value);
            }
        }
        request
    }

    pub(super) fn chat_request(&self, config: &ChannelConfig, body: &Value) -> RequestBuilder {
        let request = self
            .client
            .post(Self::endpoint(&config.base_url, "chat/completions"))
            .json(body);
        self.authorize(request, config)
    }

    pub(super) fn test_request(&self, config: &ChannelConfig) -> RequestBuilder {
        match self.preset.test {
            TestStrategy::Models => {
                let request = self.client.get(Self::endpoint(&config.base_url, "models"));
                self.authorize(request, config)
            }
            TestStrategy::Chat => {
                let model = config
                    .models
                    .first()
                    .map(String::as_str)
                    .unwrap_or(self.preset.models[0]);
                self.chat_request(
                    config,
                    &json!({
                        "model": self.resolve_model(config, model),
                        "messages": [{ "role": "user", "content": "Hi" }],
                        "max_tokens": 1,
                        "stream": false
                    }),
                )
            }
        }
    }

    fn upstream_error(&self, status: StatusCode, body: &str) -> String {
        let message = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| body.to_owned());
        format!(
            "{} 上游返回 HTTP {}: {}",
            self.preset.name,
            status.as_u16(),
            message
        )
    }

    pub(super) fn extract_usage(body: &Value) -> Option<TokenUsage> {
        let usage = body.get("usage")?;
        Some(TokenUsage {
            prompt_tokens: usage.get("prompt_tokens")?.as_u64()?,
            completion_tokens: usage.get("completion_tokens")?.as_u64()?,
            total_tokens: usage.get("total_tokens")?.as_u64()?,
        })
    }
}

#[async_trait]
impl Adapter for OpenAiCompatible {
    async fn test(&self, config: &ChannelConfig) -> anyhow::Result<TestResult> {
        let started = Instant::now();
        let response = match self.test_request(config).send().await {
            Ok(response) => response,
            Err(error) => {
                return Ok(TestResult {
                    success: false,
                    message: format!("无法连接 {} 上游: {error}", self.preset.name),
                    latency_ms: started.elapsed().as_millis() as u64,
                })
            }
        };
        let status = response.status();
        let latency_ms = started.elapsed().as_millis() as u64;
        let message = if status.is_success() {
            format!("{} 渠道连接成功", self.preset.name)
        } else {
            self.upstream_error(status, &response.text().await.unwrap_or_default())
        };
        Ok(TestResult {
            success: status.is_success(),
            message,
            latency_ms,
        })
    }

    async fn forward(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> anyhow::Result<ProxyResponse> {
        let body = self.build_body(config, request, false)?;
        let response = self
            .chat_request(config, &body)
            .send()
            .await
            .with_context(|| format!("请求 {} 上游失败", self.preset.name))?;
        let status = response.status().as_u16();
        let text = response.text().await.context("读取上游响应失败")?;
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        let usage = Self::extract_usage(&body);
        Ok(ProxyResponse {
            status,
            body,
            usage,
        })
    }

    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> anyhow::Result<reqwest::Response> {
        let body = self.build_body(config, request, true)?;
        self.chat_request(config, &body)
            .send()
            .await
            .with_context(|| format!("请求 {} 流式上游失败", self.preset.name))
    }
}
