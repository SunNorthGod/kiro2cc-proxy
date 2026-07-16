// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! API Key 用量追踪模块
//!
//! 记录每个 API Key 的请求用量（input/output tokens），并根据模型定价估算费用。
//! 数据持久化到 `api_key_usage.json`。

use chrono::{DateTime, FixedOffset, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 单条用量记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    /// API Key ID（0 = 主密钥）
    pub api_key_id: u32,
    /// 账号 ID（None 表示旧数据或未知）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<u64>,
    /// 模型名称
    pub model: String,
    /// 输入 tokens
    pub input_tokens: i32,
    /// 输出 tokens
    pub output_tokens: i32,
    /// 估算费用（美元）
    pub estimated_cost: f64,
    /// 真实 credits 消耗（来自 meteringEvent，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_used: Option<f64>,
    /// 缓存命中的输入 token 数（来自 meteringEvent 或反推，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i32>,
    /// 缓存创建的输入 token 数（来自 meteringEvent，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i32>,
    /// 5m ephemeral tier 的 cache_creation 拆分（默认 0，向后兼容）
    #[serde(default)]
    pub cache_creation_5m_input_tokens: i32,
    /// 1h ephemeral tier 的 cache_creation 拆分（默认 0，向后兼容）
    #[serde(default)]
    pub cache_creation_1h_input_tokens: i32,
    /// 记录时间
    pub created_at: DateTime<Utc>,
    /// 客户端 IP（None 表示旧数据或未知）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    /// 中转对接来源（Some(中转名) 表示此请求由外部中转服务承接，credits 为估算值）。
    /// None 表示由 Kiro 承接（正常情况）。用于把 relay 记录从 GPT 计费自标定中排除，
    /// 避免自身估算值反过来污染标定系数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
}

// ============ GPT 计费基准（自标定） ============
//
// GPT-5 官方定价（美元/百万 token），仅用于计算“官方美元成本”作为中间量；
// 真正的 credits 由 `官方USD × k × 倍率` 得出，其中 k 由线上真实 Kiro GPT 记录自标定。
const GPT_INPUT_PER_MTOK: f64 = 1.25;
const GPT_CACHE_READ_PER_MTOK: f64 = 0.125;
const GPT_CACHE_WRITE_PER_MTOK: f64 = 1.5625;
const GPT_OUTPUT_PER_MTOK: f64 = 10.0;

/// 当线上样本不足时的兜底 credits/USD（实测 2026-07 ≈ 27.8）。
pub const GPT_FALLBACK_CREDIT_PER_USD: f64 = 27.8;

/// 用 GPT 官方定价结构计算一批 token 的“官方美元成本”。
/// `fresh_input` 为不含缓存的纯输入 token。
pub fn gpt_official_usd(fresh_input: i64, cache_read: i64, cache_creation: i64, output: i64) -> f64 {
    fresh_input.max(0) as f64 / 1e6 * GPT_INPUT_PER_MTOK
        + cache_read.max(0) as f64 / 1e6 * GPT_CACHE_READ_PER_MTOK
        + cache_creation.max(0) as f64 / 1e6 * GPT_CACHE_WRITE_PER_MTOK
        + output.max(0) as f64 / 1e6 * GPT_OUTPUT_PER_MTOK
}

/// 单个 API Key 的用量汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    /// API Key ID
    pub api_key_id: u32,
    /// 总请求次数
    pub total_requests: u64,
    /// 总输入 tokens
    pub total_input_tokens: i64,
    /// 总输出 tokens
    pub total_output_tokens: i64,
    /// 总估算费用（美元）
    pub total_cost: f64,
    /// 总真实 credits 消耗（credits_used，缺失则按 estimated_cost*k_ref 估算）
    #[serde(default)]
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
    /// 按模型分组的用量
    pub by_model: Vec<ModelUsage>,
}

/// 按模型分组的用量
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub model: String,
    pub requests: u64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost: f64,
    /// 真实 credits 消耗（credits_used，缺失则按 estimated_cost*k_ref 估算）——
    /// 前端直接展示此字段，勿再用 cost 自行换算（如旧的 cost/0.72）。
    #[serde(default)]
    pub credits: f64,
}
/// 模型定价（每百万 tokens，美元）
/// 使用 200K context 标准定价
struct ModelPricing {
    input_per_mtok: f64,
    output_per_mtok: f64,
}

/// 根据模型名获取定价
fn get_model_pricing(model: &str) -> ModelPricing {
    let model_lower = model.to_lowercase();

    if model_lower.contains("opus") {
        // Opus 4.5+: $5 / $25
        ModelPricing {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
        }
    } else if model_lower.contains("haiku") {
        // Haiku 4.5: $1 / $5
        ModelPricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
        }
    } else {
        // Sonnet 系列（含 sonnet-4 / 4.5 / 4.6 / 5）统一 $3 / $15。
        // Anthropic Sonnet 各版本 token 定价一致；sonnet-5 的 credit 溢价通过
        // rateMultiplier=1.3 体现，已反映在 get_k_ref 的换算率中（见下）。
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        }
    }
}

/// 平台级 credits/USD 换算率，按模型档位差异化（代理实测 2026-06-25）。
/// 仅 usage 报表 credits_saved 字段使用（estimated_cost × k_ref - credits_used）。
/// cache_read 派生已切换为前缀估算路径，不再依赖此值。
/// 2026-06-30 重校：按 opus 版本分档，基于实测 d=0.50 缓存折扣反推。
fn get_k_ref(model: &str) -> f64 {
    let m = model.to_lowercase();
    if m.contains("opus-4-7") || m.contains("opus-4.7")
        || m.contains("opus-4-8") || m.contains("opus-4.8")
    {
        // opus 4.7/4.8 共用同档（实测 4.8 ≈ 2.36，4.7 暂沿用 4.8）
        2.36
    } else if m.contains("opus-4-5") || m.contains("opus-4.5")
        || m.contains("opus-4-6") || m.contains("opus-4.6")
    {
        // 旧 opus 4.5/4.6（实测 4.6 ≈ 1.90）
        1.90
    } else if m.contains("opus") || m.contains("fable") {
        // 未知 opus / fable 兜底沿用最新档
        2.36
    } else {
        // sonnet 系列（默认）/ haiku / sonnet-5 共用此兜底换算率。
        // 注：sonnet-5 已在 Kiro 后端上线（rateMultiplier=1.3），但 k_ref 仅用于
        // 「meteringEvent 缺失时」的 credits 兜底估算——正常计费直接取官方 metering，
        // 已含 1.3x 溢价，故此处无需为 sonnet-5 单列常量；如需精确可待有真实
        // metering 样本后按实测反推校准。
        1.43
    }
}

/// 计算单次请求的估算费用
fn calculate_cost(model: &str, input_tokens: i32, output_tokens: i32) -> f64 {
    let pricing = get_model_pricing(model);
    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    input_cost + output_cost
}

/// 每个 API Key / 账号的最大日志条数，超出时删除最老的记录
const MAX_RECORDS_PER_KEY: usize = 10_000;

/// 用量追踪器（线程安全）
pub struct UsageTracker {
    records: Arc<RwLock<Vec<UsageRecord>>>,

    dirty_tx: mpsc::UnboundedSender<()>,
}
impl UsageTracker {
    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let records = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };
        let records = Arc::new(RwLock::new(records));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let records_clone = records.clone();
        let path_clone = path.clone();

        // 启动后台异步写入任务，避免同步文件写阻塞请求线程
        tokio::spawn(async move {
            let mut dirty = false;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    res = rx.recv() => {
                        match res {
                            Some(_) => dirty = true,
                            None => {
                                // 通道已关闭（系统退出），执行 Graceful Shutdown 刷盘
                                if dirty
                                    && let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                        tracing::error!("Graceful shutdown usage save failed: {}", e);
                                    }
                                break;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        if dirty {
                            if let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                tracing::error!("Failed to save usage: {}", e);
                            } else {
                                dirty = false;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            records,

            dirty_tx: tx,
        })
    }

    /// 内部真正的异步落地方法
    async fn save_internal(
        records: &Arc<RwLock<Vec<UsageRecord>>>,
        file_path: &Path,
    ) -> anyhow::Result<()> {
        let content = {
            let r = records.read();
            serde_json::to_string(&*r)?
        };
        let path = file_path.to_path_buf();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, content)?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    /// 记录一次请求用量
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        api_key_id: u32,
        credential_id: Option<u64>,
        model: String,
        input_tokens: i32,
        output_tokens: i32,
        client_ip: Option<String>,
        credits_used: Option<f64>,
        cache_read_input_tokens: Option<i32>,
        cache_creation_input_tokens: Option<i32>,
    ) {
        let cost = calculate_cost(&model, input_tokens, output_tokens);
        let record = UsageRecord {
            api_key_id,
            credential_id,
            model,
            input_tokens,
            output_tokens,
            estimated_cost: cost,
            credits_used,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
            created_at: Utc::now(),
            client_ip,
            relay: None,
        };
        {
            let mut records = self.records.write();
            records.push(record);

            // 按 api_key_id 裁剪：保留最新的 MAX_RECORDS_PER_KEY 条
            let key_count = records
                .iter()
                .filter(|r| r.api_key_id == api_key_id)
                .count();
            if key_count > MAX_RECORDS_PER_KEY {
                let excess = key_count - MAX_RECORDS_PER_KEY;
                let mut removed = 0;
                records.retain(|r| {
                    if removed < excess && r.api_key_id == api_key_id {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                });
            }

            // 按 credential_id 裁剪
            if let Some(cid) = credential_id {
                let cred_count = records
                    .iter()
                    .filter(|r| r.credential_id == Some(cid))
                    .count();
                if cred_count > MAX_RECORDS_PER_KEY {
                    let excess = cred_count - MAX_RECORDS_PER_KEY;
                    let mut removed = 0;
                    records.retain(|r| {
                        if removed < excess && r.credential_id == Some(cid) {
                            removed += 1;
                            false
                        } else {
                            true
                        }
                    });
                }
            }
        }
        let _ = self.dirty_tx.send(());
    }

    /// 记录一次由外部中转承接的请求用量。
    ///
    /// 与 `record` 的区别：`relay` 字段标记来源中转名；`credits_used` 为按 GPT 自标定
    /// 基准估算的 credits（已含倍率），会被正常计入 API Key 额度扣减，但会从后续的
    /// GPT 计费自标定中排除，避免估算值自我强化。`model` 记录真实中转模型名（不伪装）。
    #[allow(clippy::too_many_arguments)]
    pub fn record_relay(
        &self,
        api_key_id: u32,
        model: String,
        input_tokens: i32,
        output_tokens: i32,
        client_ip: Option<String>,
        credits_used: f64,
        cache_read_input_tokens: i32,
        cache_creation_input_tokens: i32,
        relay_name: String,
    ) {
        let record = UsageRecord {
            api_key_id,
            credential_id: None,
            model,
            input_tokens,
            output_tokens,
            estimated_cost: 0.0,
            credits_used: Some(credits_used),
            cache_read_input_tokens: Some(cache_read_input_tokens),
            cache_creation_input_tokens: Some(cache_creation_input_tokens),
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
            created_at: Utc::now(),
            client_ip,
            relay: Some(relay_name),
        };
        {
            let mut records = self.records.write();
            records.push(record);
            let key_count = records
                .iter()
                .filter(|r| r.api_key_id == api_key_id)
                .count();
            if key_count > MAX_RECORDS_PER_KEY {
                let excess = key_count - MAX_RECORDS_PER_KEY;
                let mut removed = 0;
                records.retain(|r| {
                    if removed < excess && r.api_key_id == api_key_id {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                });
            }
        }
        let _ = self.dirty_tx.send(());
    }

    /// 按中转名聚合：累计承接请求数、累计计费 credits、最近 60s RPM。
    pub fn relay_summary(&self, relay_name: &str) -> (u64, f64, u64) {
        let records = self.records.read();
        let now = Utc::now();
        let mut requests = 0u64;
        let mut credits = 0.0;
        let mut rpm = 0u64;
        for r in records.iter() {
            if r.relay.as_deref() == Some(relay_name) {
                requests += 1;
                credits += r.credits_used.unwrap_or(0.0);
                if (now - r.created_at).num_seconds() < 60 {
                    rpm += 1;
                }
            }
        }
        (requests, credits, rpm)
    }

    /// GPT 计费自标定：扫描线上真实 Kiro GPT 记录（排除 relay 估算记录），
    /// 用 GPT 官方定价结构算出总“官方美元成本”，与真实 credits_used 求比值，
    /// 得到该部署自身的 credits/USD 系数 k。样本不足（USD 或 credits 为 0）返回 None。
    pub fn gpt_credit_per_usd(&self) -> Option<f64> {
        let records = self.records.read();
        let mut total_usd = 0.0;
        let mut total_credits = 0.0;
        for r in records.iter() {
            if r.relay.is_some() {
                continue;
            }
            if !r.model.to_lowercase().starts_with("gpt") {
                continue;
            }
            let Some(cu) = r.credits_used else {
                continue;
            };
            let cache_read = r.cache_read_input_tokens.unwrap_or(0).max(0) as i64;
            let cache_creation = r.cache_creation_input_tokens.unwrap_or(0).max(0) as i64;
            // input_tokens 含缓存，纯输入 = input - cache_read - cache_creation
            let fresh = (r.input_tokens.max(0) as i64 - cache_read - cache_creation).max(0);
            let output = r.output_tokens.max(0) as i64;
            total_usd += gpt_official_usd(fresh, cache_read, cache_creation, output);
            total_credits += cu;
        }
        if total_usd > 0.0 && total_credits > 0.0 {
            Some(total_credits / total_usd)
        } else {
            None
        }
    }

    /// 获取单个 API Key 的用量汇总
    pub fn get_summary(&self, api_key_id: u32) -> UsageSummary {
        let records = self.records.read();
        let filtered: Vec<&UsageRecord> = records
            .iter()
            .filter(|r| r.api_key_id == api_key_id)
            .collect();

        let mut by_model: HashMap<String, (u64, i64, i64, f64, f64)> = HashMap::new();
        for r in &filtered {
            let entry = by_model.entry(r.model.clone()).or_default();
            entry.0 += 1;
            entry.1 += r.input_tokens as i64;
            entry.2 += r.output_tokens as i64;
            entry.3 += r.estimated_cost;
            entry.4 += r
                .credits_used
                .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model));
        }

        let total_credits_saved: f64 = filtered
            .iter()
            .filter_map(|r| {
                // relay 记录 estimated_cost=0，算出的“省”是负的假值，排除
                if r.relay.is_some() {
                    return None;
                }
                r.credits_used
                    .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu)
            })
            .sum();

        let total_credits: f64 = filtered
            .iter()
            .map(|r| {
                r.credits_used
                    .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model))
            })
            .sum();

        UsageSummary {
            api_key_id,
            total_requests: filtered.len() as u64,
            total_input_tokens: filtered.iter().map(|r| r.input_tokens as i64).sum(),
            total_output_tokens: filtered.iter().map(|r| r.output_tokens as i64).sum(),
            total_cost: filtered.iter().map(|r| r.estimated_cost).sum(),
            total_credits,
            total_credits_saved,
            by_model: by_model
                .into_iter()
                .map(|(model, (requests, input, output, cost, credits))| ModelUsage {
                    model,
                    requests,
                    input_tokens: input,
                    output_tokens: output,
                    cost,
                    credits,
                })
                .collect(),
        }
    }

    /// 获取所有 API Key 的用量概览
    pub fn get_all_summaries(&self) -> Vec<UsageSummary> {
        let records = self.records.read();
        let mut key_ids: Vec<u32> = records.iter().map(|r| r.api_key_id).collect();
        key_ids.sort();
        key_ids.dedup();
        drop(records);

        key_ids.iter().map(|&id| self.get_summary(id)).collect()
    }

    /// 重置指定 API Key 的用量记录
    pub fn reset(&self, api_key_id: u32) -> anyhow::Result<()> {
        let mut records = self.records.write();
        records.retain(|r| r.api_key_id != api_key_id);
        drop(records);
        let _ = self.dirty_tx.send(());
        Ok(())
    }

    /// 获取指定 API Key 的累计费用（美元估算，内部保留；全链路已改用 credits）
    #[allow(dead_code)]
    pub fn get_total_cost(&self, api_key_id: u32) -> f64 {
        let records = self.records.read();
        records
            .iter()
            .filter(|r| r.api_key_id == api_key_id)
            .map(|r| r.estimated_cost)
            .sum()
    }

    /// 获取指定 API Key 的累计真实 credits 消耗。
    ///
    /// 优先使用 meteringEvent 上报的真实 `credits_used`；对于没有该字段的
    /// 旧记录，回退到 `estimated_cost * get_k_ref(model)` 估算，与用量报表口径一致。
    pub fn get_total_credits(&self, api_key_id: u32) -> f64 {
        let records = self.records.read();
        records
            .iter()
            .filter(|r| r.api_key_id == api_key_id)
            .map(|r| {
                r.credits_used
                    .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model))
            })
            .sum()
    }

    /// 分页查询指定 API Key 的原始请求记录（按 created_at 降序）
    /// page 从 1 开始，小于 1 的值视为 1
    /// credential_labels: 账号 ID -> 显示标签（email 或 nickname）
    pub fn get_records_paged(
        &self,
        api_key_id: u32,
        page: usize,
        page_size: usize,
        credential_labels: &HashMap<u64, String>,
    ) -> UsageRecordsPage {
        if page_size == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size: 0,
                total_pages: 0,
            };
        }

        // 在锁内只做过滤和克隆，不做排序
        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.api_key_id == api_key_id)
                .cloned()
                .collect()
        };

        let total = owned.len();
        if total == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        // 锁已释放，在锁外排序
        let mut sorted = owned;
        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let total_pages = total.div_ceil(page_size);
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let items: Vec<UsageRecordItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|r| {
                let credential_label = r
                    .credential_id
                    .and_then(|cid| credential_labels.get(&cid).cloned());
                let credits_saved = if r.relay.is_some() {
                    None
                } else {
                    r.credits_used
                        .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu)
                };
                let credits = r
                    .credits_used
                    .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model));
                UsageRecordItem {
                    credits,
                    model: r.model,
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    estimated_cost: r.estimated_cost,
                    credits_used: r.credits_used,
                    credits_saved,
                    cache_read_input_tokens: r.cache_read_input_tokens,
                    cache_creation_input_tokens: r.cache_creation_input_tokens,
                    created_at: r.created_at,
                    credential_id: r.credential_id,
                    credential_label,
                    client_ip: r.client_ip,
                    relay: r.relay,
                }
            })
            .collect();

        UsageRecordsPage {
            records: items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}

/// 分页查询结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecordsPage {
    pub records: Vec<UsageRecordItem>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// 对外暴露的单条记录
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecordItem {
    pub model: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub estimated_cost: f64,
    /// 计费用 credits（credits_used，缺失则按 estimated_cost*k_ref 估算）——
    /// 前端直接展示此字段，勿再自行换算（如旧的 estimatedCost/0.72）。
    pub credits: f64,
    /// 真实 credits 消耗（来自 meteringEvent，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_used: Option<f64>,
    /// 缓存命中的输入 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i32>,
    /// 缓存创建的输入 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i32>,
    /// 节省的 credits（与无缓存对比）= estimated_cost * get_k_ref(model) - credits_used
    /// 仅当 credits_used 有值时才有值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_saved: Option<f64>,
    pub created_at: DateTime<Utc>,
    /// 使用的账号 ID（None 表示旧数据或主密钥请求）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<u64>,
    /// 账号账号（email 或 nickname，用于前端显示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_label: Option<String>,
    /// 客户端 IP（None 表示旧数据或未知）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    /// 中转来源（Some(中转名) 表示此请求由外部中转承接，credits 为估算值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
}

impl UsageTracker {
    /// 分页查询指定账号的原始请求记录（按 created_at 降序）
    pub fn get_records_paged_by_credential(
        &self,
        credential_id: u64,
        page: usize,
        page_size: usize,
        credential_labels: &HashMap<u64, String>,
    ) -> UsageRecordsPage {
        if page_size == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size: 0,
                total_pages: 0,
            };
        }

        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.credential_id == Some(credential_id))
                .cloned()
                .collect()
        };

        let total = owned.len();
        if total == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        let mut sorted = owned;
        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let total_pages = total.div_ceil(page_size);
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let items: Vec<UsageRecordItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|r| {
                let credential_label = r
                    .credential_id
                    .and_then(|cid| credential_labels.get(&cid).cloned());
                let credits_saved = if r.relay.is_some() {
                    None
                } else {
                    r.credits_used
                        .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu)
                };
                let credits = r
                    .credits_used
                    .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model));
                UsageRecordItem {
                    credits,
                    model: r.model,
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    estimated_cost: r.estimated_cost,
                    credits_used: r.credits_used,
                    credits_saved,
                    cache_read_input_tokens: r.cache_read_input_tokens,
                    cache_creation_input_tokens: r.cache_creation_input_tokens,
                    created_at: r.created_at,
                    credential_id: r.credential_id,
                    credential_label,
                    client_ip: r.client_ip,
                    relay: r.relay,
                }
            })
            .collect();

        UsageRecordsPage {
            records: items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}

/// 按日期汇总的用量
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySummary {
    pub date: String,
    pub total_requests: u64,
    pub total_cost: f64,
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
    /// 总输入 tokens（用于历史 token 趋势图）
    #[serde(default)]
    pub total_input_tokens: i64,
    /// 总输出 tokens
    #[serde(default)]
    pub total_output_tokens: i64,
}

/// 首页概览：全历史总量 + 近 N 天序列 + 模型分布 + 按 API Key 汇总（单次扫描聚合）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewResponse {
    pub all_time: OverviewTotals,
    /// 近 N 天，按日期升序
    pub daily: Vec<DailySummary>,
    /// 全历史按模型分布（按 credits 降序）
    pub by_model: Vec<ModelUsage>,
    /// 全历史按 API Key 汇总（按 credits 降序，供 Top 排行）
    pub by_api_key: Vec<ApiKeyUsageBrief>,
}

/// 全历史累计总量
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OverviewTotals {
    pub total_requests: u64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_credits: f64,
    pub total_credits_saved: f64,
    pub total_cache_read_tokens: i64,
    pub total_cache_creation_tokens: i64,
}

/// 单个 API Key 的精简汇总（首页 Top 排行用）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyUsageBrief {
    pub api_key_id: u32,
    pub requests: u64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub credits: f64,
}

/// 指定账号在指定 CST 日期的用量汇总
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDaySummary {
    pub date: String,
    pub credential_id: u64,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: f64,
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
}

impl UsageTracker {
    /// 按 CST（UTC+8）当前日期聚合指定 credential 的用量。
    ///
    /// 返回结构包含今日的请求数、输入/输出 token、估算费用、credits 用量及节省值。
    /// 当 credential 在今日没有记录时返回零值汇总（不报错）。
    pub fn get_today_summary_for_credential(
        &self,
        credential_id: u64,
    ) -> CredentialDaySummary {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let today = chrono::Utc::now()
            .with_timezone(&cst)
            .format("%Y-%m-%d")
            .to_string();

        let mut requests: u64 = 0;
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let mut cost: f64 = 0.0;
        let mut credits: f64 = 0.0;
        let mut credits_saved: f64 = 0.0;

        let records = self.records.read();
        for r in records.iter() {
            if r.credential_id != Some(credential_id) {
                continue;
            }
            let date = r
                .created_at
                .with_timezone(&cst)
                .format("%Y-%m-%d")
                .to_string();
            if date != today {
                continue;
            }
            requests += 1;
            input_tokens = input_tokens.saturating_add(r.input_tokens.max(0) as u64);
            output_tokens = output_tokens.saturating_add(r.output_tokens.max(0) as u64);
            cost += r.estimated_cost;
            let k_ref = get_k_ref(&r.model);
            credits += r.credits_used.unwrap_or(r.estimated_cost * k_ref);
            if let Some(cu) = r.credits_used {
                credits_saved += r.estimated_cost * k_ref - cu;
            }
        }

        CredentialDaySummary {
            date: today,
            credential_id,
            total_requests: requests,
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
            total_cost: cost,
            total_credits: credits,
            total_credits_saved: credits_saved,
        }
    }

    /// 按 CST（UTC+8）日期聚合所有记录，返回按日期降序的汇总列表
    pub fn get_daily_summaries(&self) -> Vec<DailySummary> {
        use std::collections::BTreeMap;
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let records = self.records.read();
        let mut map: BTreeMap<String, (u64, f64, f64, f64, i64, i64)> = BTreeMap::new();
        for r in records.iter() {
            let date = r
                .created_at
                .with_timezone(&cst)
                .format("%Y-%m-%d")
                .to_string();
            let entry = map.entry(date).or_default();
            entry.0 += 1;
            entry.1 += r.estimated_cost;
            entry.2 += r
                .credits_used
                .unwrap_or(r.estimated_cost * get_k_ref(&r.model));
            if r.relay.is_none()
                && let Some(cu) = r.credits_used
            {
                entry.3 += r.estimated_cost * get_k_ref(&r.model) - cu;
            }
            entry.4 += r.input_tokens as i64;
            entry.5 += r.output_tokens as i64;
        }
        let mut result: Vec<DailySummary> = map
            .into_iter()
            .map(|(date, (reqs, cost, credits, saved, it, ot))| DailySummary {
                date,
                total_requests: reqs,
                total_cost: cost,
                total_credits: credits,
                total_credits_saved: saved,
                total_input_tokens: it,
                total_output_tokens: ot,
            })
            .collect();
        result.sort_by(|a, b| b.date.cmp(&a.date));
        result
    }

    /// 首页概览聚合：单次扫描产出全历史总量 + 近 `daily_days` 天序列 + 模型分布 + API Key 汇总。
    pub fn get_overview(&self, daily_days: usize) -> OverviewResponse {
        use std::collections::BTreeMap;
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let records = self.records.read();

        let mut totals = OverviewTotals::default();
        let mut by_day: BTreeMap<String, (u64, f64, f64, f64, i64, i64)> = BTreeMap::new();
        let mut by_model: HashMap<String, (u64, i64, i64, f64, f64)> = HashMap::new();
        let mut by_key: HashMap<u32, (u64, i64, i64, f64)> = HashMap::new();

        for r in records.iter() {
            let k_ref = get_k_ref(&r.model);
            let credits = r.credits_used.unwrap_or(r.estimated_cost * k_ref);
            let saved = if r.relay.is_some() {
                0.0
            } else {
                r.credits_used
                    .map(|cu| r.estimated_cost * k_ref - cu)
                    .unwrap_or(0.0)
            };
            let it = r.input_tokens as i64;
            let ot = r.output_tokens as i64;

            totals.total_requests += 1;
            totals.total_input_tokens += it;
            totals.total_output_tokens += ot;
            totals.total_credits += credits;
            totals.total_credits_saved += saved;
            totals.total_cache_read_tokens += r.cache_read_input_tokens.unwrap_or(0) as i64;
            totals.total_cache_creation_tokens +=
                r.cache_creation_input_tokens.unwrap_or(0) as i64;

            let date = r
                .created_at
                .with_timezone(&cst)
                .format("%Y-%m-%d")
                .to_string();
            let d = by_day.entry(date).or_default();
            d.0 += 1;
            d.1 += r.estimated_cost;
            d.2 += credits;
            d.3 += saved;
            d.4 += it;
            d.5 += ot;

            let m = by_model.entry(r.model.clone()).or_default();
            m.0 += 1;
            m.1 += it;
            m.2 += ot;
            m.3 += r.estimated_cost;
            m.4 += credits;

            let k = by_key.entry(r.api_key_id).or_default();
            k.0 += 1;
            k.1 += it;
            k.2 += ot;
            k.3 += credits;
        }
        drop(records);

        // BTreeMap 已按日期升序；取最近 daily_days 天
        let mut daily: Vec<DailySummary> = by_day
            .into_iter()
            .map(|(date, (reqs, cost, credits, saved, it, ot))| DailySummary {
                date,
                total_requests: reqs,
                total_cost: cost,
                total_credits: credits,
                total_credits_saved: saved,
                total_input_tokens: it,
                total_output_tokens: ot,
            })
            .collect();
        if daily.len() > daily_days {
            daily = daily.split_off(daily.len() - daily_days);
        }

        let mut by_model_v: Vec<ModelUsage> = by_model
            .into_iter()
            .map(|(model, (requests, input, output, cost, credits))| ModelUsage {
                model,
                requests,
                input_tokens: input,
                output_tokens: output,
                cost,
                credits,
            })
            .collect();
        by_model_v.sort_by(|a, b| b.credits.partial_cmp(&a.credits).unwrap_or(std::cmp::Ordering::Equal));

        let mut by_key_v: Vec<ApiKeyUsageBrief> = by_key
            .into_iter()
            .map(|(api_key_id, (requests, input, output, credits))| ApiKeyUsageBrief {
                api_key_id,
                requests,
                input_tokens: input,
                output_tokens: output,
                credits,
            })
            .collect();
        by_key_v.sort_by(|a, b| b.credits.partial_cmp(&a.credits).unwrap_or(std::cmp::Ordering::Equal));

        OverviewResponse {
            all_time: totals,
            daily,
            by_model: by_model_v,
            by_api_key: by_key_v,
        }
    }

    /// 分页查询指定 CST（UTC+8）日期的原始记录，硬限总量 2000 条
    pub fn get_records_paged_by_date(
        &self,
        date: &str,
        page: usize,
        page_size: usize,
        credential_labels: &std::collections::HashMap<u64, String>,
    ) -> UsageRecordsPage {
        const MAX_TOTAL: usize = 2000;
        let page_size = page_size.clamp(1, 500);
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();

        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| {
                    r.created_at
                        .with_timezone(&cst)
                        .format("%Y-%m-%d")
                        .to_string()
                        == date
                })
                .cloned()
                .collect()
        };

        let mut sorted = owned;
        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        sorted.truncate(MAX_TOTAL);

        let total = sorted.len();
        if total == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        let total_pages = total.div_ceil(page_size);
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let items: Vec<UsageRecordItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|r| {
                let credential_label = r
                    .credential_id
                    .and_then(|cid| credential_labels.get(&cid).cloned());
                let credits_saved = if r.relay.is_some() {
                    None
                } else {
                    r.credits_used
                        .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu)
                };
                let credits = r
                    .credits_used
                    .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model));
                UsageRecordItem {
                    credits,
                    model: r.model,
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    estimated_cost: r.estimated_cost,
                    credits_used: r.credits_used,
                    credits_saved,
                    cache_read_input_tokens: r.cache_read_input_tokens,
                    cache_creation_input_tokens: r.cache_creation_input_tokens,
                    created_at: r.created_at,
                    credential_id: r.credential_id,
                    credential_label,
                    client_ip: r.client_ip,
                    relay: r.relay,
                }
            })
            .collect();

        UsageRecordsPage {
            records: items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}
