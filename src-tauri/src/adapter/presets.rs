//! 供应商默认值与测试策略；用户配置的地址、模型映射优先。
pub(super) enum TestStrategy {
    Models,
    Chat,
}

pub(super) struct ProviderPreset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub models: &'static [&'static str],
    pub model_mapping: &'static [(&'static str, &'static str)],
    pub extra_headers: &'static [(&'static str, &'static str)],
    pub test: TestStrategy,
}

pub(super) const OPENAI: ProviderPreset = ProviderPreset {
    name: "OpenAI",
    base_url: "https://api.openai.com/v1",
    models: &["gpt-4o", "gpt-4o-mini"],
    model_mapping: &[],
    extra_headers: &[
        ("organization", "OpenAI-Organization"),
        ("project", "OpenAI-Project"),
    ],
    test: TestStrategy::Models,
};

// 保留项目现有 edgefn 预设，不把第三方模型别名混入协议实现。
pub(super) const DEEPSEEK: ProviderPreset = ProviderPreset {
    name: "DeepSeek",
    base_url: "https://api.edgefn.net/v1",
    models: &["DeepSeekV4Pro"],
    model_mapping: &[("DeepSeekV4Pro", "DeepSeek-V4-Pro-0813")],
    extra_headers: &[],
    test: TestStrategy::Chat,
};

pub(super) fn preset(channel_type: &str) -> Option<&'static ProviderPreset> {
    match channel_type {
        "openai" => Some(&OPENAI),
        "deepseek" => Some(&DEEPSEEK),
        _ => None,
    }
}
