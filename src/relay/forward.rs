// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 中转转发：请求体改写（透传 + 剥 thinking 签名）、流式/非流式转发、计费入库

use std::sync::Arc;

use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};
use futures::{StreamExt, stream};
use serde_json::{Value, json};

use crate::model::usage::{UsageTracker, gpt_official_usd};

use super::manager::RelayManager;
use super::types::RelayConfig;

/// 从中转响应嗅探到的用量累加器
#[derive(Debug, Clone, Default)]
struct UsageAcc {
    /// 纯输入 token（Anthropic input_tokens，不含缓存）
    input: i64,
    cache_read: i64,
    cache_creation: i64,
    output: i64,
}

/// 计费上下文（随流式状态一起搬运）
struct BillCtx {
    tracker: Option<Arc<UsageTracker>>,
    api_key_id: Option<u32>,
    client_ip: Option<String>,
    model: String,
    relay_name: String,
    multiplier: f64,
    gpt_k: f64,
}

fn to_i32(v: i64) -> i32 {
    v.clamp(0, i32::MAX as i64) as i32
}

impl BillCtx {
    fn record(&self, acc: &UsageAcc) {
        let (Some(tracker), Some(kid)) = (&self.tracker, self.api_key_id) else {
            return;
        };
        let usd = gpt_official_usd(acc.input, acc.cache_read, acc.cache_creation, acc.output);
        let credits = usd * self.gpt_k * self.multiplier;
        let input_incl_cache = acc.input + acc.cache_read + acc.cache_creation;
        tracing::info!(
            "[relay-usage] 入库: relay={} model={} input(incl_cache)={} cache_read={} cache_creation={} output={} usd={:.6} k={:.3} x{:.2} credits={:.4} api_key={}",
            self.relay_name,
            self.model,
            input_incl_cache,
            acc.cache_read,
            acc.cache_creation,
            acc.output,
            usd,
            self.gpt_k,
            self.multiplier,
            credits,
            kid,
        );
        tracker.record_relay(
            kid,
            self.model.clone(),
            to_i32(input_incl_cache),
            to_i32(acc.output),
            self.client_ip.clone(),
            credits,
            to_i32(acc.cache_read),
            to_i32(acc.cache_creation),
            self.relay_name.clone(),
        );
    }
}

/// 从一条已解析的 SSE / JSON 值里提取 usage 累加到 acc（取最新的正值）。
fn extract_usage(v: &Value, acc: &mut UsageAcc) {
    let usage = v
        .get("message")
        .and_then(|m| m.get("usage"))
        .or_else(|| v.get("usage"));
    if let Some(u) = usage {
        if let Some(x) = u.get("input_tokens").and_then(|x| x.as_i64()) {
            if x > 0 {
                acc.input = x;
            }
        }
        if let Some(x) = u.get("cache_read_input_tokens").and_then(|x| x.as_i64()) {
            if x > 0 {
                acc.cache_read = x;
            }
        }
        if let Some(x) = u
            .get("cache_creation_input_tokens")
            .and_then(|x| x.as_i64())
        {
            if x > 0 {
                acc.cache_creation = x;
            }
        }
        if let Some(x) = u.get("output_tokens").and_then(|x| x.as_i64()) {
            if x > 0 {
                acc.output = x; // message_delta 的 output 为累计值，末值即终值
            }
        }
    }
}

/// 增量嗅探 SSE 文本里的 `data:` 行，累加 usage（透传字节不受影响）。
fn sniff_sse(buf: &mut String, chunk: &[u8], acc: &mut UsageAcc) {
    buf.push_str(&String::from_utf8_lossy(chunk));
    while let Some(idx) = buf.find('\n') {
        let line: String = buf[..idx].trim().to_string();
        buf.drain(..=idx);
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                extract_usage(&v, acc);
            }
        }
    }
}

/// 剥离单条消息里的历史 thinking 块与 reasoningContent（避免跨厂商签名 400）。
fn strip_thinking_blocks(msg: &mut Value) {
    if let Some(obj) = msg.as_object_mut() {
        obj.remove("reasoningContent");
        if let Some(content) = obj.get_mut("content") {
            if let Some(arr) = content.as_array_mut() {
                arr.retain(|b| {
                    let t = b.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    t != "thinking" && t != "redacted_thinking"
                });
            }
        }
    }
}

/// 把原始 Anthropic 请求体改写为发往中转的请求体：
/// - model → target；stream → 指定值
/// - 去掉 Kiro 私有 / 跨厂商不兼容字段（thinking / output_config / anthropic_beta）
/// - 剥历史 thinking 块与签名（tool_use / tool_result 保持透传，由中转转 OpenAI）
fn transform_body(original: &[u8], target: &str, stream: bool) -> Value {
    let mut v: Value = serde_json::from_slice(original).unwrap_or_else(|_| json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("model".into(), json!(target));
        obj.insert("stream".into(), json!(stream));
        obj.remove("thinking");
        obj.remove("output_config");
        obj.remove("anthropic_beta");
        if let Some(msgs) = obj.get_mut("messages").and_then(|m| m.as_array_mut()) {
            for msg in msgs.iter_mut() {
                strip_thinking_blocks(msg);
            }
        }
    }
    v
}

/// 拉取中转的模型列表（GET /v1/models，Anthropic/OpenAI 通用 data[].id）
pub async fn fetch_models(
    manager: &RelayManager,
    relay: &RelayConfig,
) -> anyhow::Result<Vec<String>> {
    let url = format!("{}/v1/models", relay.normalized_base());
    let resp = manager
        .probe_client()?
        .get(&url)
        .header("x-api-key", &relay.api_key)
        .header(header::AUTHORIZATION, format!("Bearer {}", relay.api_key))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("连接中转失败: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "中转 /v1/models 返回 {}: {}",
            status,
            text.chars().take(300).collect::<String>()
        );
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("解析中转模型列表失败: {}", e))?;
    let mut out = Vec::new();
    if let Some(data) = v.get("data").and_then(|d| d.as_array()) {
        for m in data {
            if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                out.push(id.to_string());
            }
        }
    }
    if out.is_empty() {
        anyhow::bail!("中转未返回任何模型");
    }
    Ok(out)
}

/// 转发一次请求到中转。成功返回 Response；失败返回 Err（由调用方决定回落策略）。
#[allow(clippy::too_many_arguments)]
pub async fn forward(
    manager: &RelayManager,
    relay: &RelayConfig,
    target_model: &str,
    original_body: &[u8],
    stream: bool,
    usage_tracker: Option<Arc<UsageTracker>>,
    api_key_id: Option<u32>,
    client_ip: Option<String>,
    gpt_k: f64,
) -> anyhow::Result<Response> {
    let body = transform_body(original_body, target_model, stream);
    let url = format!("{}/v1/messages", relay.normalized_base());

    let resp = manager
        .client()
        .post(&url)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-api-key", &relay.api_key)
        .header(header::AUTHORIZATION, format!("Bearer {}", relay.api_key))
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("中转请求失败: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "中转返回错误 {}: {}",
            status,
            text.chars().take(300).collect::<String>()
        );
    }

    let bill = BillCtx {
        tracker: usage_tracker,
        api_key_id,
        client_ip,
        model: target_model.to_string(),
        relay_name: relay.name.clone(),
        multiplier: relay.billing_multiplier,
        gpt_k,
    };

    if stream {
        let byte_stream = resp.bytes_stream();
        let out = stream::unfold(
            (byte_stream, String::new(), UsageAcc::default(), bill, false),
            |(mut bs, mut buf, mut acc, bill, done)| async move {
                if done {
                    return None;
                }
                match bs.next().await {
                    Some(Ok(chunk)) => {
                        sniff_sse(&mut buf, &chunk, &mut acc);
                        Some((
                            Ok::<_, std::convert::Infallible>(chunk),
                            (bs, buf, acc, bill, false),
                        ))
                    }
                    Some(Err(e)) => {
                        tracing::warn!("中转流式读取中断: {}", e);
                        bill.record(&acc);
                        None
                    }
                    None => {
                        bill.record(&acc);
                        None
                    }
                }
            },
        );

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(Body::from_stream(out))
            .unwrap())
    } else {
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("读取中转响应失败: {}", e))?;
        let mut acc = UsageAcc::default();
        if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
            extract_usage(&v, &mut acc);
        }
        bill.record(&acc);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(bytes))
            .unwrap())
    }
}
