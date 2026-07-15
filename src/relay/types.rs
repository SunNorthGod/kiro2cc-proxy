// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 中转对接（备用路由）数据结构

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}
fn default_multiplier() -> f64 {
    1.0
}

/// 路由触发模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteMode {
    /// 直连：命中该规则的模型直接走中转，不经过 Kiro
    Direct,
    /// 兜底：仅当 Kiro 账号池整体失败时才走中转
    Fallback,
}

/// 单条路由规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    /// 请求模型匹配模式（支持 `*` 通配，如 `gpt*`、`claude-*`、`*`）
    pub pattern: String,
    /// 目标模型（中转 /v1/models 里的真实模型名）
    pub target: String,
    /// 触发模式
    pub mode: RouteMode,
}

/// 单个中转配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfig {
    /// 唯一 ID
    pub id: u64,
    /// 名称 / 备注
    pub name: String,
    /// 基础地址（如 https://ai.example.com:2053，末尾不带 /v1）
    pub base_url: String,
    /// 中转 API Key
    pub api_key: String,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 缓存的模型列表（来自最近一次“拉取模型”）
    #[serde(default)]
    pub models: Vec<String>,
    /// 路由规则
    #[serde(default)]
    pub routes: Vec<RouteRule>,
    /// 计费倍率：credits = 官方USD × 自标定k × 倍率（默认 1.0 = 与 Kiro 真实 GPT 同价）
    #[serde(default = "default_multiplier")]
    pub billing_multiplier: f64,
    /// 创建时间
    #[serde(default)]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl RelayConfig {
    /// 规范化 base_url：去掉末尾 `/`，若误填了 `/v1` 结尾则一并去掉。
    pub fn normalized_base(&self) -> String {
        let mut b = self.base_url.trim().trim_end_matches('/').to_string();
        if let Some(stripped) = b.strip_suffix("/v1") {
            b = stripped.to_string();
        }
        b
    }

    /// 对外脱敏视图（隐藏完整 api_key）
    pub fn to_masked(&self) -> RelayView {
        let masked = mask_key(&self.api_key);
        RelayView {
            id: self.id,
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            masked_api_key: masked,
            enabled: self.enabled,
            models: self.models.clone(),
            routes: self.routes.clone(),
            billing_multiplier: self.billing_multiplier,
            created_at: self.created_at,
        }
    }
}

fn mask_key(key: &str) -> String {
    let n = key.chars().count();
    if n <= 8 {
        "*".repeat(n)
    } else {
        let head: String = key.chars().take(4).collect();
        let tail: String = key.chars().skip(n - 4).collect();
        format!("{}...{}", head, tail)
    }
}

/// 对外脱敏视图
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayView {
    pub id: u64,
    pub name: String,
    pub base_url: String,
    pub masked_api_key: String,
    pub enabled: bool,
    pub models: Vec<String>,
    pub routes: Vec<RouteRule>,
    pub billing_multiplier: f64,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 创建中转请求
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRelayRequest {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub routes: Vec<RouteRule>,
    #[serde(default = "default_multiplier")]
    pub billing_multiplier: f64,
}

/// 更新中转请求（字段缺省表示不改；api_key 为空字符串表示不改）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRelayRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub routes: Option<Vec<RouteRule>>,
    #[serde(default)]
    pub billing_multiplier: Option<f64>,
}

/// 测试连接 / 拉取模型的响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayModelsResponse {
    pub models: Vec<String>,
}

/// 简单 glob 匹配：仅支持 `*` 通配符（可出现在任意位置、任意个数）。
/// 大小写不敏感。空 pattern 不匹配任何东西。
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.trim().to_lowercase();
    let text = text.to_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    // 无通配符 → 精确匹配
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // 首段必须是前缀
            if !text[pos..].starts_with(part) {
                return false;
            }
            pos += part.len();
        } else if i == parts.len() - 1 {
            // 末段必须是后缀
            return text[pos..].ends_with(part);
        } else {
            match text[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
    }
    true
}
