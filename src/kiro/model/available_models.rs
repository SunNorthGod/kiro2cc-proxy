// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 可用模型查询数据模型
//!
//! 包含 ListAvailableModels API 的响应类型定义。
//! 端点（实测）：`GET https://management.{region}.kiro.dev/ListAvailableModels`
//! `?origin=AI_EDITOR&profileArn=...`（external_idp 走 management，其余走
//! `q.{region}.amazonaws.com`）。注意：必须用 GET，POST 返回 400
//! `UnknownOperationException`。响应单页返回（实测无 nextToken），但仍按
//! 分页协议解析以防后端启用分页。

use serde::Deserialize;

/// ListAvailableModels 响应
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAvailableModelsResponse {
    /// 可用模型列表
    #[serde(default)]
    pub models: Vec<KiroModelInfo>,

    /// 默认模型（通常为 `auto`）
    #[serde(default)]
    pub default_model: Option<KiroModelInfo>,

    /// 分页游标（实测为空）
    #[serde(default)]
    pub next_token: Option<String>,
}

/// 单个模型信息（仅保留中转用得到的字段）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroModelInfo {
    /// Kiro 模型 ID，点分格式，如 `claude-sonnet-4.5` / `claude-sonnet-5`
    #[serde(default)]
    pub model_id: Option<String>,

    /// 展示名，如 `Claude Sonnet 5`
    #[serde(default)]
    pub model_name: Option<String>,

    /// 描述文案
    #[serde(default)]
    pub description: Option<String>,

    /// token 上限（含上下文窗口与最大输出）
    #[serde(default)]
    pub token_limits: Option<TokenLimits>,

    /// credit 倍率（如 sonnet-5=1.3，opus-4.8=2.2）。仅信息展示用，
    /// 中转实际计费取 meteringEvent，不依赖此字段。保留以备后续展示/校准。
    #[serde(default)]
    #[allow(dead_code)]
    pub rate_multiplier: Option<f64>,

    /// 模型专属请求参数 schema。effort 档位信息藏在这里：
    /// `properties.output_config.properties.effort.{enum,default}`
    /// （或 `properties.reasoning.properties.effort.{...}`）。
    /// 实测各模型 effort 档位不一致：sonnet-5/opus-4.8 有 xhigh，opus-4.6/sonnet-4.6 无 xhigh，
    /// 其余（sonnet-4.5/haiku/minimax 等）无 effort schema（不支持思考档位）。
    #[serde(default)]
    pub additional_model_request_fields_schema: Option<serde_json::Value>,
}

/// 从模型 schema 中提取的 effort 档位信息（对齐 Kiro 官方 output_config/reasoning.effort）。
#[derive(Debug, Clone)]
pub struct EffortInfo {
    /// schema 路径：`output_config` 或 `reasoning`
    pub schema_path: String,
    /// 合法档位列表（如 [low, medium, high, xhigh, max]）
    pub levels: Vec<String>,
    /// 默认档位
    pub default_level: Option<String>,
}

impl KiroModelInfo {
    /// 解析该模型支持的 effort 档位（无则返回 None，表示不支持思考档位）。
    /// 逻辑对齐 Kiro 客户端：依次探测 output_config / reasoning 两条 schema 路径下的
    /// `effort.enum`（非空即命中）。
    pub fn effort_info(&self) -> Option<EffortInfo> {
        let schema = self.additional_model_request_fields_schema.as_ref()?;
        let props = schema.get("properties")?;
        for path in ["output_config", "reasoning"] {
            let effort = props
                .get(path)
                .and_then(|p| p.get("properties"))
                .and_then(|p| p.get("effort"));
            let Some(effort) = effort else { continue };
            let levels: Vec<String> = effort
                .get("enum")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if levels.is_empty() {
                continue;
            }
            let default_level = effort
                .get("default")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            return Some(EffortInfo {
                schema_path: path.to_string(),
                levels,
                default_level,
            });
        }
        None
    }
}

/// 模型 token 上限
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenLimits {
    /// 最大输入 token（即上下文窗口）
    #[serde(default)]
    pub max_input_tokens: Option<i32>,

    /// 最大输出 token
    #[serde(default)]
    pub max_output_tokens: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_with_effort(enum_vals: &str, default: &str) -> KiroModelInfo {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "output_config": {
                    "type": "object",
                    "properties": {
                        "effort": { "type": "string", "enum": serde_json::from_str::<serde_json::Value>(enum_vals).unwrap(), "default": default }
                    }
                }
            }
        });
        KiroModelInfo {
            model_id: Some("claude-sonnet-5".into()),
            model_name: Some("Claude Sonnet 5".into()),
            description: None,
            token_limits: None,
            rate_multiplier: None,
            additional_model_request_fields_schema: Some(schema),
        }
    }

    #[test]
    fn test_effort_info_extracts_levels_and_default() {
        let m = model_with_effort(r#"["low","medium","high","xhigh","max"]"#, "high");
        let eff = m.effort_info().expect("should have effort");
        assert_eq!(eff.schema_path, "output_config");
        assert_eq!(eff.levels, vec!["low", "medium", "high", "xhigh", "max"]);
        assert_eq!(eff.default_level.as_deref(), Some("high"));
    }

    #[test]
    fn test_effort_info_none_when_no_schema() {
        let m = KiroModelInfo {
            model_id: Some("claude-sonnet-4.5".into()),
            model_name: None,
            description: None,
            token_limits: None,
            rate_multiplier: None,
            additional_model_request_fields_schema: None,
        };
        assert!(m.effort_info().is_none());
    }

    #[test]
    fn test_effort_info_reasoning_path() {
        let schema = serde_json::json!({
            "properties": {
                "reasoning": {
                    "properties": {
                        "effort": { "enum": ["low", "high"], "default": "high" }
                    }
                }
            }
        });
        let m = KiroModelInfo {
            model_id: Some("x".into()),
            model_name: None,
            description: None,
            token_limits: None,
            rate_multiplier: None,
            additional_model_request_fields_schema: Some(schema),
        };
        let eff = m.effort_info().expect("reasoning path");
        assert_eq!(eff.schema_path, "reasoning");
        assert_eq!(eff.levels, vec!["low", "high"]);
    }
}
