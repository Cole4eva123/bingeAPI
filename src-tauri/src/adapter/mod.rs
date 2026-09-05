//! 上游协议接口。适配器只接收连接配置，不依赖数据库或 Tauri。
mod openai_compatible;
mod presets;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use openai_compatible::OpenAiCompatible;
use presets::preset;

/// 仅在后端使用，避免自动 Debug/Serialize 输出密钥。
pub struct ChannelConfig {
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub model_mapping: HashMap<String, String>,
    pub extra: Value,
}

pub struct ProxyRequest {
    pub model: String,
    pub body: Value,
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug)]
pub struct ProxyResponse {
    pub status: u16,
    pub body: Value,
    pub usage: Option<TokenUsage>,
}

#[async_trait]
pub trait Adapter: Send + Sync {
    /// 上游拒绝或连接失败返回 success=false；内部流程错误返回 Err。
    async fn test(&self, config: &ChannelConfig) -> anyhow::Result<TestResult>;
    /// 保留上游 4xx/5xx 响应；网络或读取错误返回 Err。
    async fn forward(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> anyhow::Result<ProxyResponse>;
    /// 返回尚未消费的响应，由调用方读取并转发 SSE。
    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> anyhow::Result<reqwest::Response>;
}

pub fn validate_channel_type(channel_type: &str) -> AppResult<()> {
    preset(channel_type)
        .map(|_| ())
        .ok_or_else(|| AppError::InvalidInput(format!("暂不支持的渠道类型: {channel_type}")))
}

/// Client 的 clone 共享连接池，所有 command 使用启动时创建的同一个客户端。
pub fn get_adapter(channel_type: &str, client: &reqwest::Client) -> AppResult<Box<dyn Adapter>> {
    let preset = preset(channel_type)
        .ok_or_else(|| AppError::InvalidInput(format!("暂不支持的渠道类型: {channel_type}")))?;
    Ok(Box::new(OpenAiCompatible::new(client.clone(), preset)))
}
