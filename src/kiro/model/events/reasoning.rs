// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 推理内容事件
//!
//! 处理 reasoningContentEvent 类型的事件。
//!
//! Kiro/CodeWhisperer 对于具备原生扩展思考能力的模型（如 opus-4.8）会将推理
//! 内容通过独立的 `reasoningContentEvent` 帧下发，而非内联在 `assistantResponseEvent`
//! 的 `<thinking>` 标签里。每个帧可能携带一段推理文本 `text`，或推理签名 `signature`。
//! 若不解析此事件，opus 等模型的推理内容会被整体丢弃，导致客户端「思考」块不显示。

use serde::{Deserialize, Serialize};

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// 推理内容事件
///
/// 承载模型原生推理（extended thinking）的增量内容。
///
/// # 字段
/// - `text`：一段推理文本增量（可能为空或缺失）
/// - `signature`：推理签名（通常在推理结束时下发一次）
///
/// 其余未使用字段通过 `#[serde(flatten)]` 捕获，确保反序列化兼容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningContentEvent {
    /// 推理文本增量
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// 推理签名（模型下发的真实签名）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,

    /// 捕获其他未使用的字段，确保反序列化兼容性
    #[serde(flatten)]
    #[serde(skip_serializing)]
    #[allow(dead_code)]
    extra: serde_json::Value,
}

impl ReasoningContentEvent {
    /// 构造推理事件（主要用于测试）
    pub fn new(text: Option<String>, signature: Option<String>) -> Self {
        Self {
            text,
            signature,
            extra: serde_json::Value::Null,
        }
    }

    /// 推理文本（无则返回空串）
    pub fn text_str(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }

    /// 推理签名（无则返回 None）
    pub fn signature_str(&self) -> Option<&str> {
        self.signature.as_deref().filter(|s| !s.is_empty())
    }
}

impl EventPayload for ReasoningContentEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}

impl Default for ReasoningContentEvent {
    fn default() -> Self {
        Self {
            text: None,
            signature: None,
            extra: serde_json::Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_text() {
        let json = r#"{"text":"Let me think"}"#;
        let event: ReasoningContentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.text_str(), "Let me think");
        assert_eq!(event.signature_str(), None);
    }

    #[test]
    fn test_deserialize_signature() {
        let json = r#"{"signature":"EvYBCmMIDxAB"}"#;
        let event: ReasoningContentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.text_str(), "");
        assert_eq!(event.signature_str(), Some("EvYBCmMIDxAB"));
    }

    #[test]
    fn test_deserialize_empty_signature_is_none() {
        let json = r#"{"signature":""}"#;
        let event: ReasoningContentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.signature_str(), None);
    }

    #[test]
    fn test_deserialize_with_extra_fields() {
        let json = r#"{"text":"hmm","conversationId":"c-1","foo":"bar"}"#;
        let event: ReasoningContentEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.text_str(), "hmm");
    }
}
