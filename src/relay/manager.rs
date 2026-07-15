// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 中转对接管理器：存储 / CRUD / 持久化 / 路由解析 / 计费系数缓存

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};

use crate::http_client::{self, ProxyConfig};
use crate::model::config::TlsBackend;
use crate::model::usage::{GPT_FALLBACK_CREDIT_PER_USD, UsageTracker};

use super::types::{CreateRelayRequest, RelayConfig, RouteMode, UpdateRelayRequest, glob_match};

/// 命中的路由目标
#[derive(Debug, Clone)]
pub struct RouteTarget {
    pub relay: RelayConfig,
    pub target_model: String,
}

/// GPT credits/USD 自标定缓存 TTL
const CALIBRATION_TTL: Duration = Duration::from_secs(300);

/// 中转对接管理器
pub struct RelayManager {
    relays: RwLock<Vec<RelayConfig>>,
    path: PathBuf,
    next_id: AtomicU64,
    client: reqwest::Client,
    tls_backend: TlsBackend,
    proxy: Option<ProxyConfig>,
    /// (标定时刻, k) —— credits/USD 自标定缓存
    calib_cache: Mutex<Option<(Instant, f64)>>,
}

impl RelayManager {
    /// 从文件加载中转配置，文件不存在则空列表
    pub fn load(
        path: impl AsRef<Path>,
        proxy: Option<ProxyConfig>,
        tls_backend: TlsBackend,
    ) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let relays: Vec<RelayConfig> = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };
        let max_id = relays.iter().map(|r| r.id).max().unwrap_or(0);
        // 中转出站 client：读空闲超时给足（长流式），建连超时短
        let client = http_client::build_client(proxy.as_ref(), 300, tls_backend)?;
        Ok(Self {
            relays: RwLock::new(relays),
            path,
            next_id: AtomicU64::new(max_id + 1),
            client,
            tls_backend,
            proxy,
            calib_cache: Mutex::new(None),
        })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    fn persist(&self) {
        let content = {
            let guard = self.relays.read();
            serde_json::to_string_pretty(&*guard)
        };
        match content {
            Ok(json) => {
                if let Some(parent) = self.path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&self.path, json) {
                    tracing::error!("保存中转配置失败: {}", e);
                }
            }
            Err(e) => tracing::error!("序列化中转配置失败: {}", e),
        }
    }

    /// 列出所有中转（原始，含明文 key，仅内部使用）
    pub fn list(&self) -> Vec<RelayConfig> {
        self.relays.read().clone()
    }

    pub fn get(&self, id: u64) -> Option<RelayConfig> {
        self.relays.read().iter().find(|r| r.id == id).cloned()
    }

    pub fn create(&self, req: CreateRelayRequest) -> RelayConfig {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cfg = RelayConfig {
            id,
            name: req.name,
            base_url: req.base_url,
            api_key: req.api_key,
            enabled: req.enabled,
            models: Vec::new(),
            routes: req.routes,
            billing_multiplier: if req.billing_multiplier > 0.0 {
                req.billing_multiplier
            } else {
                1.0
            },
            created_at: Some(chrono::Utc::now()),
        };
        self.relays.write().push(cfg.clone());
        self.persist();
        cfg
    }

    pub fn update(&self, id: u64, req: UpdateRelayRequest) -> anyhow::Result<RelayConfig> {
        let updated = {
            let mut guard = self.relays.write();
            let cfg = guard
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or_else(|| anyhow::anyhow!("中转 #{} 不存在", id))?;
            if let Some(v) = req.name {
                cfg.name = v;
            }
            if let Some(v) = req.base_url {
                cfg.base_url = v;
            }
            // api_key 仅在非空时更新（前端不回填明文，空表示保持不变）
            if let Some(v) = req.api_key {
                if !v.trim().is_empty() {
                    cfg.api_key = v;
                }
            }
            if let Some(v) = req.enabled {
                cfg.enabled = v;
            }
            if let Some(v) = req.routes {
                cfg.routes = v;
            }
            if let Some(v) = req.billing_multiplier {
                if v > 0.0 {
                    cfg.billing_multiplier = v;
                }
            }
            cfg.clone()
        };
        self.persist();
        Ok(updated)
    }

    pub fn delete(&self, id: u64) -> anyhow::Result<()> {
        {
            let mut guard = self.relays.write();
            let before = guard.len();
            guard.retain(|r| r.id != id);
            if guard.len() == before {
                anyhow::bail!("中转 #{} 不存在", id);
            }
        }
        self.persist();
        Ok(())
    }

    /// 覆盖某中转缓存的模型列表（拉取模型后调用）
    pub fn set_models(&self, id: u64, models: Vec<String>) {
        {
            let mut guard = self.relays.write();
            if let Some(cfg) = guard.iter_mut().find(|r| r.id == id) {
                cfg.models = models;
            }
        }
        self.persist();
    }

    /// 查找命中 direct 规则的目标（仅启用的中转）。
    pub fn match_direct(&self, model: &str) -> Option<RouteTarget> {
        self.match_mode(model, RouteMode::Direct)
    }

    /// 查找命中 fallback 规则的目标（仅启用的中转）。
    pub fn match_fallback(&self, model: &str) -> Option<RouteTarget> {
        self.match_mode(model, RouteMode::Fallback)
    }

    fn match_mode(&self, model: &str, mode: RouteMode) -> Option<RouteTarget> {
        let guard = self.relays.read();
        for relay in guard.iter() {
            if !relay.enabled {
                continue;
            }
            for rule in &relay.routes {
                if rule.mode == mode && glob_match(&rule.pattern, model) {
                    // target 留空 = 透传原模型号
                    let target_model = if rule.target.trim().is_empty() {
                        model.to_string()
                    } else {
                        rule.target.clone()
                    };
                    return Some(RouteTarget {
                        relay: relay.clone(),
                        target_model,
                    });
                }
            }
        }
        None
    }

    /// 获取（缓存的）GPT credits/USD 自标定系数；样本不足时用兜底常量。
    pub fn gpt_credit_per_usd(&self, tracker: &Arc<UsageTracker>) -> f64 {
        {
            let guard = self.calib_cache.lock();
            if let Some((ts, k)) = guard.as_ref() {
                if ts.elapsed() < CALIBRATION_TTL {
                    return *k;
                }
            }
        }
        let k = tracker
            .gpt_credit_per_usd()
            .unwrap_or(GPT_FALLBACK_CREDIT_PER_USD);
        *self.calib_cache.lock() = Some((Instant::now(), k));
        k
    }

    /// 构建一个临时 client（用于测试连接，超时短）
    pub fn probe_client(&self) -> anyhow::Result<reqwest::Client> {
        http_client::build_client(self.proxy.as_ref(), 30, self.tls_backend)
    }
}
