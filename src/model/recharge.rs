// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! API Key 充值（续费/开卡）流水追踪模块
//!
//! 与 `usage.rs`（消费记录）对称：记录每张 Key 的每一次额度/时长变动
//! （开卡初始额度、后续续费），数据持久化到 `api_key_recharge.json`。
//! 计费消耗看 usage，进账（充值）看这里。

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 单条充值流水
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RechargeRecord {
    /// 目标 API Key ID
    pub api_key_id: u32,
    /// 类型：`create`（开卡初始额度）/ `topup`（续费充值）
    pub kind: String,
    /// 本次增加的额度（credits）。仅额度卡有值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_credits: Option<f64>,
    /// 本次增加的时长（天）。仅时长卡有值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_days: Option<f64>,
    /// 充值后的总额度（credits），无额度限制时为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit_limit_after: Option<f64>,
    /// 充值后的到期时间（懒激活未激活卡为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_after: Option<DateTime<Utc>>,
    /// 来源：`admin`（管理员）/ `reseller`（分销商）
    pub source: String,
    /// 备注（可选，如操作者、订单号等）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// 记录时间
    pub created_at: DateTime<Utc>,
}

/// 每张 Key 的最大充值流水条数，超出时删除最老的记录
const MAX_RECORDS_PER_KEY: usize = 10_000;

/// 充值流水追踪器（线程安全，结构对齐 UsageTracker）
pub struct RechargeTracker {
    records: Arc<RwLock<Vec<RechargeRecord>>>,
    dirty_tx: mpsc::UnboundedSender<()>,
}

impl RechargeTracker {
    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let records: Vec<RechargeRecord> = if path.exists() {
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

        // 后台异步落盘任务，避免同步写阻塞请求线程（与 UsageTracker 一致）
        tokio::spawn(async move {
            let mut dirty = false;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    res = rx.recv() => {
                        match res {
                            Some(_) => dirty = true,
                            None => {
                                if dirty
                                    && let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                        tracing::error!("Graceful shutdown recharge save failed: {}", e);
                                    }
                                break;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        if dirty {
                            if let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                tracing::error!("Failed to save recharge: {}", e);
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

    async fn save_internal(
        records: &Arc<RwLock<Vec<RechargeRecord>>>,
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

    /// 记录一次充值/开卡流水
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        api_key_id: u32,
        kind: &str,
        add_credits: Option<f64>,
        add_days: Option<f64>,
        credit_limit_after: Option<f64>,
        expires_at_after: Option<DateTime<Utc>>,
        source: &str,
        note: Option<String>,
    ) {
        let record = RechargeRecord {
            api_key_id,
            kind: kind.to_string(),
            add_credits,
            add_days,
            credit_limit_after,
            expires_at_after,
            source: source.to_string(),
            note,
            created_at: Utc::now(),
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
        }
        let _ = self.dirty_tx.send(());
    }

    /// 分页查询指定 API Key 的充值流水（按 created_at 降序）。
    /// page 从 1 开始，小于 1 视为 1；page_size 为 0 返回空页。
    pub fn get_records_paged(
        &self,
        api_key_id: u32,
        page: usize,
        page_size: usize,
    ) -> RechargeRecordsPage {
        if page_size == 0 {
            return RechargeRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size: 0,
                total_pages: 0,
            };
        }

        let mut owned: Vec<RechargeRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.api_key_id == api_key_id)
                .cloned()
                .collect()
        };

        let total = owned.len();
        if total == 0 {
            return RechargeRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        owned.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let total_pages = total.div_ceil(page_size);
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let records: Vec<RechargeRecord> =
            owned.into_iter().skip(start).take(page_size).collect();

        RechargeRecordsPage {
            records,
            total,
            page,
            page_size,
            total_pages,
        }
    }

    /// 清除指定 API Key 的充值流水（删除子卡密时调用，避免残留）
    pub fn reset(&self, api_key_id: u32) {
        {
            let mut records = self.records.write();
            records.retain(|r| r.api_key_id != api_key_id);
        }
        let _ = self.dirty_tx.send(());
    }
}

/// 分页查询结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RechargeRecordsPage {
    pub records: Vec<RechargeRecord>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_tracker() -> RechargeTracker {
        // 用不存在的临时路径，落盘任务不影响测试断言（只读内存）
        let dir = std::env::temp_dir();
        let path = dir.join(format!("recharge_test_{}.json", uuid::Uuid::new_v4()));
        RechargeTracker::load(path).unwrap()
    }

    #[tokio::test]
    async fn test_record_and_paged_desc() {
        let t = tmp_tracker();
        t.record(1, "create", Some(1000.0), None, Some(1000.0), None, "admin", None);
        t.record(1, "topup", Some(500.0), None, Some(1500.0), None, "admin", None);
        t.record(2, "create", Some(200.0), None, Some(200.0), None, "reseller", None);

        let page = t.get_records_paged(1, 1, 50);
        assert_eq!(page.total, 2);
        assert_eq!(page.records.len(), 2);
        // 降序：最新的 topup 在前
        assert_eq!(page.records[0].kind, "topup");
        assert_eq!(page.records[0].credit_limit_after, Some(1500.0));
        assert_eq!(page.records[1].kind, "create");

        let page2 = t.get_records_paged(2, 1, 50);
        assert_eq!(page2.total, 1);
        assert_eq!(page2.records[0].source, "reseller");
    }

    #[tokio::test]
    async fn test_reset_clears_key() {
        let t = tmp_tracker();
        t.record(9, "create", Some(100.0), None, Some(100.0), None, "reseller", None);
        assert_eq!(t.get_records_paged(9, 1, 50).total, 1);
        t.reset(9);
        assert_eq!(t.get_records_paged(9, 1, 50).total, 0);
    }

    #[tokio::test]
    async fn test_pagination_bounds() {
        let t = tmp_tracker();
        for i in 0..5 {
            t.record(3, "topup", Some(i as f64), None, None, None, "admin", None);
        }
        let p = t.get_records_paged(3, 2, 2);
        assert_eq!(p.total, 5);
        assert_eq!(p.total_pages, 3);
        assert_eq!(p.page, 2);
        assert_eq!(p.records.len(), 2);
        // page_size 0 → 空
        assert_eq!(t.get_records_paged(3, 1, 0).total, 0);
    }
}
