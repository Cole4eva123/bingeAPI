use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type ExtraConfig = Map<String, Value>;
pub type ModelMapping = HashMap<String, String>;

/// 后端使用的渠道；密钥不参与前端数据序列化。
#[derive(Clone)]
pub struct Channel {
    pub info: ChannelInfo,
    pub api_key: String,
}

/// 查询和写入渠道统一返回此结构，JSON 字段始终是数组或对象。
#[derive(Debug, Clone, Serialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    pub base_url: String,
    pub has_api_key: bool,
    pub models: Vec<String>,
    pub status: i64,
    pub priority: i64,
    pub weight: i64,
    pub config: ExtraConfig,
    pub model_mapping: ModelMapping,
    pub created_at: String,
    pub updated_at: String,
    pub last_test_at: Option<String>,
    pub last_test_ok: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateChannelInput {
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub config: Option<ExtraConfig>,
    pub model_mapping: Option<ModelMapping>,
}

/// 未传或为 null 的字段保持原值；空对象/数组表示清空对应配置。
#[derive(Default, Deserialize)]
pub struct UpdateChannelInput {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub channel_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub models: Option<Vec<String>>,
    pub status: Option<i64>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub config: Option<ExtraConfig>,
    pub model_mapping: Option<ModelMapping>,
}
