//! 上游渠道适配器模块。
//!
//! 本模块负责与各家 LLM 供应商的 HTTP API 交互，和数据库层无关。
//! 主要包含以下内容：
//!
//! 1. **`Adaptor` trait** — 所有供应商适配器的统一接口；
//! 2. **共享 DTO** — `ChannelConfig`、`ProxyRequest`、`TestResult`、`TokenUsage`，
//!    描述适配器层需要的输入和输出数据结构；
//! 3. **工厂函数 `get_adaptor`** — 根据渠道类型字符串创建对应的适配器实例；
//! 4. **具体适配器实现** — `openai` 和 `deepseek` 子模块。
//!
//! 业务层通过 `get_adaptor` 拿到 `Box<dyn Adaptor>`，即可统一调用
//! `test`、`forward`、`forward_stream`，无需关心底层供应商差异。

pub mod deepseek;
pub mod openai;

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 共享数据结构（DTO）
// ---------------------------------------------------------------------------

/// 渠道配置——从数据库 `Channel` 记录转换而来。
///
/// 适配器只需要知道"怎么连上游"，不需要知道数据库 schema，
/// 因此用这个精简的配置结构体代替直接依赖 `Channel` 模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// 上游 API 根地址，例如 `https://api.openai.com/v1`。
    pub base_url: String,
    /// 上游 API 密钥，用于 Bearer 认证。
    pub api_key: String,
    /// 该渠道支持的模型列表。
    pub models: Vec<String>,
    /// 模型名称映射：key 是客户端请求的模型名，value 是上游真正的模型名。
    pub model_mapping: HashMap<String, String>,
    /// 供应商特有的扩展配置，例如 OpenAI 的 organization / project。
    pub extra: serde_json::Value,
}

/// 代理请求——统一的上游请求抽象。
///
/// 业务层将客户端请求归一化为 `ProxyRequest` 后交给适配器，
/// 适配器再根据各自供应商的协议将其转换为实际的 HTTP 请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequest {
    /// 客户端请求的模型名称（可能需要经过 model_mapping 转换）。
    pub model: String,
    /// 请求体中的动态 JSON 数据，例如 `messages`、`temperature`、`tools` 等。
    ///
    /// 不同上游支持的字段不完全相同，所以这里不使用固定结构体；适配器可以用
    /// `body.get("messages")` 读取字段，也可以直接将 `body` 序列化后发送给上游。
    pub body: serde_json::Value,
    /// 客户端是否请求流式响应。
    pub stream: bool,
}

/// 渠道连通性测试结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// 测试是否成功。
    pub success: bool,
    /// 人类可读的测试结果消息。
    pub message: String,
    /// 测试请求的往返延迟（毫秒）。
    pub latency_ms: u64,
}

/// Token 用量——统一各家供应商的计费格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 输入（提示）消耗的 Token 数。
    pub prompt_tokens: u64,
    /// 输出（补全）消耗的 Token 数。
    pub completion_tokens: u64,
    /// 总计消耗的 Token 数。
    pub total_tokens: u64,
}

// ---------------------------------------------------------------------------
// Adaptor trait
// ---------------------------------------------------------------------------

/// 上游渠道适配器的统一接口。
///
/// OpenAI、DeepSeek、Claude、Gemini 等供应商的地址、认证方式、请求体以及流式协议
/// 各不相同。每个供应商分别实现 `Adaptor`，业务层便可以只依赖这套统一接口，
/// 不需要在每个请求流程里反复判断供应商类型。
///
/// `Send + Sync` 要求实现类型可以安全地跨线程移动和共享。Tauri 的异步运行时
/// 可能在不同工作线程执行请求，因此准备放进共享状态的适配器需要满足这两个约束。
#[async_trait]
pub trait Adaptor: Send + Sync {
    /// 返回数据库 `channels.type` 使用的渠道类型标识，例如 `"openai"`。
    ///
    /// 返回 `&'static str` 表示该字符串在程序整个运行期间都有效，
    /// 实现时通常直接返回一个字符串常量，不需要每次创建新的 `String`。
    fn channel_type(&self) -> &'static str;

    /// 返回创建该类渠道时建议展示的默认模型列表。
    ///
    /// 这只是应用内置的初始值，不代表供应商实时、完整的模型列表；
    /// 用户仍然可以通过数据库中的 `Channel.models` 自定义模型。
    fn default_models(&self) -> Vec<&'static str>;

    /// 返回该供应商的默认 API 根地址。
    ///
    /// 实际发请求时仍然使用 `ChannelConfig.base_url`，这样才能支持用户配置的
    /// 官方地址、反向代理地址以及兼容协议的第三方服务。
    fn default_base_url(&self) -> &str;

    /// 测试渠道地址和 API Key 是否可用，并记录请求耗时。
    ///
    /// 可以预期的上游拒绝（例如 API Key 无效）应返回
    /// `Ok(TestResult { success: false, ... })`；只有无法正常完成测试流程的
    /// 内部错误才返回 `Err(anyhow::Error)`。
    async fn test(&self, config: &ChannelConfig) -> Result<TestResult, anyhow::Error>;

    /// 执行一次非流式请求。
    ///
    /// 返回值依次是：
    /// 1. 上游 HTTP 状态码；
    /// 2. 上游 JSON 响应体；
    /// 3. 上游提供的 Token 用量。错误响应或部分兼容服务可能没有用量，
    ///    因此这里使用 `Option<TokenUsage>`。
    ///
    /// HTTP 4xx/5xx 本身也应作为正常的上游响应返回，由调用方把状态码和响应体
    /// 转交给客户端；网络中断、读取失败等情况才使用 `Err`。
    async fn forward(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<(u16, serde_json::Value, Option<TokenUsage>), anyhow::Error>;

    /// 执行一次流式请求，并保留尚未读取的上游 HTTP 响应体。
    ///
    /// 调用方取得 `reqwest::Response` 后负责检查状态码、保留 SSE 响应头，
    /// 并逐块读取 body 转发给客户端。这里不能先调用 `.json()` 或 `.text()`，
    /// 否则响应流会被一次性消费，失去流式转发的意义。
    async fn forward_stream(
        &self,
        request: &ProxyRequest,
        config: &ChannelConfig,
    ) -> Result<reqwest::Response, anyhow::Error>;
}

// ---------------------------------------------------------------------------
// 适配器工厂
// ---------------------------------------------------------------------------

/// 根据数据库中的渠道类型创建对应的适配器。
///
/// 返回 `Box<dyn Adaptor>` 是因为不同供应商的适配器是不同的 Rust 类型，
/// 但它们都实现了 `Adaptor`。装进 trait object 后，业务层可以通过同一种类型
/// 调用 `test`、`forward` 和 `forward_stream`，不需要知道里面的具体实现。
///
/// 当前支持 OpenAI 和 DeepSeek。未知类型直接报错，可以防止数据库中的拼写
/// 错误被静默当成其他渠道处理。以后实现新供应商时，再在这个 `match` 中增加
/// 对应分支即可。
pub fn get_adaptor(channel_type: &str) -> anyhow::Result<Box<dyn Adaptor>> {
    match channel_type {
        "openai" => Ok(Box::new(openai::OpenAiAdapter::new())),
        "deepseek" => Ok(Box::new(deepseek::DeepSeekAdapter::new())),
        _ => Err(anyhow::anyhow!("暂不支持的渠道类型: {channel_type}")),
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod adaptor_factory_tests {
    use super::*;

    #[test]
    fn creates_openai_adaptor() {
        let adaptor = get_adaptor("openai").unwrap();

        assert_eq!(adaptor.channel_type(), "openai");
    }

    #[test]
    fn creates_deepseek_adaptor() {
        let adaptor = get_adaptor("deepseek").unwrap();

        assert_eq!(adaptor.channel_type(), "deepseek");
        assert_eq!(adaptor.default_base_url(), "https://api.edgefn.net/v1");
        assert_eq!(adaptor.default_models(), vec!["DeepSeekV4Pro"]);
    }

    #[test]
    fn rejects_unsupported_channel_type() {
        let error = get_adaptor("claude").err().unwrap();

        assert_eq!(error.to_string(), "暂不支持的渠道类型: claude");
    }
}
