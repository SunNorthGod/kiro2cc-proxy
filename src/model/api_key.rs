// Copyright (c) 2026 Harllan He. Licensed under MIT.
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 单个 API Key
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: u32,
    pub key: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// 额度限制（美元），None 表示不限额（按日期模式）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spending_limit: Option<f64>,
    /// 额度限制（真实 Kiro credits），None 表示不按 credits 限额。
    /// 与 spending_limit 相互独立，可同时设置；任一超限即拒绝。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_limit: Option<f64>,
    /// 有效期天数（懒激活模式），首次使用后才开始计时
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_days: Option<f64>,
    /// 首次使用激活时间
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<DateTime<Utc>>,
    /// 绑定的账号 ID 列表，None 或空列表表示不限制（使用全局策略）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_credential_ids: Option<Vec<u64>>,
    /// 是否为分销卡密（reseller）。分销卡密是"管理凭据"，只能用于开/管理子卡密，
    /// 禁止直接用于推理请求。其 credit_limit 即分销商的可分配预算。
    #[serde(default)]
    pub is_reseller: bool,
    /// 父分销卡密 ID（仅子卡密有值）。子卡密从父卡密的预算中预扣额度。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_key_id: Option<u32>,
    /// 已结算额度（仅分销卡密有值）。当子卡密被删除时，把它已消耗的真实 credits
    /// 累加到父卡密的 committed_credits，从而"用掉的钱不退回"，未用完的额度释放。
    #[serde(default)]
    pub committed_credits: f64,
}

fn default_enabled() -> bool {
    true
}

impl ApiKey {
    /// 生成新的 API Key
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u32,
        name: String,
        expires_at: Option<DateTime<Utc>>,
        spending_limit: Option<f64>,
        credit_limit: Option<f64>,
        duration_days: Option<f64>,
        bound_credential_ids: Option<Vec<u64>>,
    ) -> Self {
        Self {
            id,
            key: generate_api_key(),
            name,
            enabled: true,
            created_at: Utc::now(),
            expires_at,
            spending_limit,
            credit_limit,
            duration_days,
            activated_at: None,
            bound_credential_ids,
            is_reseller: false,
            parent_key_id: None,
            committed_credits: 0.0,
        }
    }

    /// 检查 key 是否有效（启用且未过期）
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if let Some(expires_at) = self.expires_at {
            return Utc::now() < expires_at;
        }
        true
    }

    /// 检查是否已过期
    /// 待激活状态（duration_days 有值但 activated_at 为 None）返回 false
    pub fn is_expired(&self) -> bool {
        if self.duration_days.is_some() && self.activated_at.is_none() {
            return false;
        }
        self.expires_at
            .map(|exp| Utc::now() >= exp)
            .unwrap_or(false)
    }

    /// 检查是否为活跃状态（已激活且未过期）
    pub fn is_active(&self) -> bool {
        self.activated_at.is_some() && !self.is_expired()
    }

    /// 激活 key：设置 activated_at 并计算 expires_at
    /// 幂等操作，已激活的 key 直接跳过
    pub fn activate(&mut self) -> bool {
        if self.activated_at.is_some() || self.duration_days.is_none() {
            return false;
        }
        let now = Utc::now();
        let days = self.duration_days.unwrap();
        let duration = chrono::Duration::milliseconds((days * 86_400_000.0) as i64);
        self.activated_at = Some(now);
        self.expires_at = Some(now + duration);
        true
    }
}
/// 生成 sk- 前缀的随机 API Key
fn generate_api_key() -> String {
    let id = uuid::Uuid::new_v4();
    format!("sk-{}", id.simple())
}

/// API Key 认证结果
pub enum ApiKeyAuthResult {
    /// 认证通过，携带 key ID 和名称
    Valid {
        id: u32,
        name: String,
        spending_limit: Option<f64>,
        credit_limit: Option<f64>,
        bound_credential_ids: Option<Vec<u64>>,
        /// 是否为分销卡密（管理凭据，禁止用于推理）
        is_reseller: bool,
    },
    /// Key 已被禁用
    Disabled,
    /// Key 已过期
    Expired,
    /// Key 不存在
    NotFound,
}

/// API Key 管理器（线程安全）
pub struct ApiKeyManager {
    keys: RwLock<Vec<ApiKey>>,
    file_path: PathBuf,
}

impl ApiKeyManager {
    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut keys: Vec<ApiKey> = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };

        // 迁移：历史上的美元额度(spending_limit)一律转为积分额度(credit_limit)。
        // 全链路只用 credits 计量，迁移后 spending_limit 永久置空。
        let mut migrated = false;
        for k in keys.iter_mut() {
            if let Some(old_usd_limit) = k.spending_limit.take() {
                if k.credit_limit.is_none() {
                    k.credit_limit = Some(old_usd_limit);
                }
                migrated = true;
            }
        }

        let manager = Self {
            keys: RwLock::new(keys),
            file_path: path,
        };
        if migrated {
            if let Err(e) = manager.save() {
                tracing::warn!("API Key 额度迁移(美元→积分)持久化失败: {}", e);
            } else {
                tracing::info!("已将历史 API Key 的美元额度迁移为积分额度");
            }
        }
        Ok(manager)
    }

    /// 持久化到文件
    fn save(&self) -> anyhow::Result<()> {
        let keys = self.keys.read();
        let content = serde_json::to_string_pretty(&*keys)?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// 验证请求中的 key
    pub fn authenticate(&self, key: &str) -> ApiKeyAuthResult {
        let keys = self.keys.read();
        match keys.iter().find(|k| k.key == key) {
            Some(api_key) => {
                if !api_key.enabled {
                    ApiKeyAuthResult::Disabled
                } else if api_key.is_expired() {
                    ApiKeyAuthResult::Expired
                } else {
                    ApiKeyAuthResult::Valid {
                        id: api_key.id,
                        name: api_key.name.clone(),
                        spending_limit: api_key.spending_limit,
                        credit_limit: api_key.credit_limit,
                        bound_credential_ids: api_key.bound_credential_ids.clone(),
                        is_reseller: api_key.is_reseller,
                    }
                }
            }
            None => ApiKeyAuthResult::NotFound,
        }
    }

    /// 只读认证：只要 key 存在就放行（不检查过期/禁用/额度）
    /// 用于用户查询用量等只读场景
    pub fn authenticate_readonly(&self, key: &str) -> ApiKeyAuthResult {
        let keys = self.keys.read();
        match keys.iter().find(|k| k.key == key) {
            Some(api_key) => ApiKeyAuthResult::Valid {
                id: api_key.id,
                name: api_key.name.clone(),
                spending_limit: api_key.spending_limit,
                credit_limit: api_key.credit_limit,
                bound_credential_ids: api_key.bound_credential_ids.clone(),
                is_reseller: api_key.is_reseller,
            },
            None => ApiKeyAuthResult::NotFound,
        }
    }
    /// 获取所有 key（克隆）
    pub fn list(&self) -> Vec<ApiKey> {
        self.keys.read().clone()
    }

    /// 创建新 key
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        name: String,
        expires_at: Option<DateTime<Utc>>,
        spending_limit: Option<f64>,
        credit_limit: Option<f64>,
        duration_days: Option<f64>,
        bound_credential_ids: Option<Vec<u64>>,
    ) -> anyhow::Result<ApiKey> {
        let mut keys = self.keys.write();
        let next_id = keys.iter().map(|k| k.id).max().unwrap_or(0) + 1;
        let api_key = ApiKey::new(
            next_id,
            name,
            expires_at,
            spending_limit,
            credit_limit,
            duration_days,
            bound_credential_ids,
        );
        keys.push(api_key.clone());
        drop(keys);
        self.save()?;
        Ok(api_key)
    }

    /// 更新 key（name, enabled, expires_at, spending_limit, duration_days）
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        expires_at: Option<Option<DateTime<Utc>>>,
        spending_limit: Option<Option<f64>>,
        credit_limit: Option<Option<f64>>,
        duration_days: Option<Option<f64>>,
        bound_credential_ids: Option<Option<Vec<u64>>>,
    ) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(api_key) = keys.iter_mut().find(|k| k.id == id) else {
            return Ok(None);
        };
        if let Some(name) = name {
            api_key.name = name;
        }
        if let Some(enabled) = enabled {
            api_key.enabled = enabled;
        }
        if let Some(expires_at) = expires_at {
            api_key.expires_at = expires_at;
        }
        if let Some(spending_limit) = spending_limit {
            api_key.spending_limit = spending_limit;
        }
        if let Some(credit_limit) = credit_limit {
            api_key.credit_limit = credit_limit;
        }
        if let Some(duration_days) = duration_days {
            match duration_days {
                Some(new_days) => {
                    if api_key.is_active() && api_key.expires_at.is_some() {
                        // 活跃 Key（有到期时间）：在当前到期时间上增量续期
                        let extension =
                            chrono::Duration::milliseconds((new_days * 86_400_000.0) as i64);
                        let new_expires = api_key.expires_at.unwrap() + extension;
                        api_key.expires_at = Some(new_expires);
                        // 重算 duration_days 为从激活到新到期的总天数
                        let total_ms =
                            (new_expires - api_key.activated_at.unwrap()).num_milliseconds();
                        api_key.duration_days = Some(total_ms as f64 / 86_400_000.0);
                    } else {
                        // 已过期或待激活：重置为待激活状态
                        api_key.duration_days = Some(new_days);
                        api_key.activated_at = None;
                        api_key.expires_at = None;
                    }
                }
                None => {
                    // 切换为"永不过期"模式
                    api_key.duration_days = None;
                    api_key.activated_at = None;
                }
            }
        }
        if let Some(ids) = bound_credential_ids {
            api_key.bound_credential_ids = ids;
        }
        let updated = api_key.clone();
        drop(keys);
        self.save()?;
        Ok(Some(updated))
    }

    /// 给 Key 增量续费：直接叠加时长（天）或额度（credits），而非覆盖设置。
    ///
    /// - `add_credits`：在现有 credit_limit 上叠加（原本无限额则从 0 起算）。
    /// - `add_days`：活跃 Key（已激活且有到期时间）在原到期时间上延长；
    ///   待激活/已过期/永久 Key 则叠加到懒激活时长（首次使用后开始计时）。
    pub fn topup(
        &self,
        id: u32,
        add_days: Option<f64>,
        add_credits: Option<f64>,
    ) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(api_key) = keys.iter_mut().find(|k| k.id == id) else {
            return Ok(None);
        };

        if let Some(c) = add_credits {
            api_key.credit_limit = Some(api_key.credit_limit.unwrap_or(0.0) + c);
        }

        if let Some(d) = add_days {
            // 永久卡密（无额度、无时长、无到期）不允许加时长，否则会被误转成时长卡
            let is_permanent = api_key.credit_limit.is_none()
                && api_key.duration_days.is_none()
                && api_key.expires_at.is_none();
            if is_permanent {
                anyhow::bail!("永久卡密不支持增加时长");
            }
            if api_key.is_active() && api_key.expires_at.is_some() {
                // 活跃 Key：在当前到期时间上延长
                let extension = chrono::Duration::milliseconds((d * 86_400_000.0) as i64);
                let new_expires = api_key.expires_at.unwrap() + extension;
                api_key.expires_at = Some(new_expires);
                if let Some(activated) = api_key.activated_at {
                    let total_ms = (new_expires - activated).num_milliseconds();
                    api_key.duration_days = Some(total_ms as f64 / 86_400_000.0);
                }
            } else {
                // 待激活/已过期/永久：叠加到懒激活时长，重置为待激活状态
                api_key.duration_days = Some(api_key.duration_days.unwrap_or(0.0) + d);
                api_key.activated_at = None;
                api_key.expires_at = None;
            }
        }

        let updated = api_key.clone();
        drop(keys);
        self.save()?;
        Ok(Some(updated))
    }

    /// 删除 key
    pub fn delete(&self, id: u32) -> anyhow::Result<bool> {
        let mut keys = self.keys.write();
        let len_before = keys.len();
        keys.retain(|k| k.id != id);
        let deleted = keys.len() < len_before;
        drop(keys);
        if deleted {
            self.save()?;
        }
        Ok(deleted)
    }

    /// 获取文件路径
    #[allow(dead_code)]
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// 激活指定 key（幂等操作）
    /// 已激活或非懒激活模式的 key 直接跳过
    pub fn activate_key(&self, id: u32) -> anyhow::Result<()> {
        let mut keys = self.keys.write();
        let Some(api_key) = keys.iter_mut().find(|k| k.id == id) else {
            return Ok(());
        };
        if api_key.activate() {
            drop(keys);
            self.save()?;
        }
        Ok(())
    }

    // ==================== 分销卡密（reseller）相关 ====================

    /// 获取指定 key 的克隆
    pub fn get(&self, id: u32) -> Option<ApiKey> {
        self.keys.read().iter().find(|k| k.id == id).cloned()
    }

    /// 列出某分销卡密的所有子卡密
    pub fn list_children(&self, parent_id: u32) -> Vec<ApiKey> {
        self.keys
            .read()
            .iter()
            .filter(|k| k.parent_key_id == Some(parent_id))
            .cloned()
            .collect()
    }

    /// 计算分销卡密的可分配余额：预算 - Σ(子卡密已分配额度) - 已结算额度。
    /// 返回 None 表示该 key 不是有效分销卡密（不存在 / 非 reseller / 无预算）。
    pub fn allocatable_credits(&self, reseller_id: u32) -> Option<f64> {
        let keys = self.keys.read();
        let reseller = keys.iter().find(|k| k.id == reseller_id)?;
        if !reseller.is_reseller {
            return None;
        }
        let budget = reseller.credit_limit?;
        let allocated: f64 = keys
            .iter()
            .filter(|k| k.parent_key_id == Some(reseller_id))
            .map(|k| k.credit_limit.unwrap_or(0.0))
            .sum();
        Some(budget - allocated - reseller.committed_credits)
    }

    /// 管理端：将某 key 标记为 / 取消分销卡密。
    /// - 标记为分销：不能是子卡密，且必须已设置额度预算(credit_limit)。
    /// - 取消分销：必须没有任何子卡密。
    pub fn set_reseller(&self, id: u32, is_reseller: bool) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(idx) = keys.iter().position(|k| k.id == id) else {
            return Ok(None);
        };
        if is_reseller {
            if keys[idx].parent_key_id.is_some() {
                anyhow::bail!("子卡密不能设为分销卡密");
            }
            if keys[idx].credit_limit.is_none() {
                anyhow::bail!("分销卡密必须先设置额度预算(credit_limit)");
            }
        } else if keys.iter().any(|c| c.parent_key_id == Some(id)) {
            anyhow::bail!("请先删除该分销卡密名下的所有子卡密");
        }
        keys[idx].is_reseller = is_reseller;
        let updated = keys[idx].clone();
        drop(keys);
        self.save()?;
        Ok(Some(updated))
    }

    /// 分销商开子卡密。原子校验可分配额度，继承父卡密的账号绑定，
    /// 到期时间受父卡密约束（见 resolve_child_schedule）。
    pub fn create_child(
        &self,
        parent_id: u32,
        name: String,
        credit_limit: f64,
        duration_days: Option<f64>,
    ) -> anyhow::Result<ApiKey> {
        if credit_limit <= 0.0 {
            anyhow::bail!("子卡密额度必须大于 0");
        }
        let mut keys = self.keys.write();
        let Some(pidx) = keys.iter().position(|k| k.id == parent_id) else {
            anyhow::bail!("分销卡密不存在");
        };
        if !keys[pidx].is_reseller {
            anyhow::bail!("该卡密不是分销卡密");
        }
        let budget = keys[pidx]
            .credit_limit
            .ok_or_else(|| anyhow::anyhow!("分销卡密未设置预算"))?;
        let allocated: f64 = keys
            .iter()
            .filter(|k| k.parent_key_id == Some(parent_id))
            .map(|k| k.credit_limit.unwrap_or(0.0))
            .sum();
        let allocatable = budget - allocated - keys[pidx].committed_credits;
        if credit_limit > allocatable + ALLOC_EPS {
            anyhow::bail!(
                "超出可分配额度：可分配 {:.2} credits，请求 {:.2} credits",
                allocatable,
                credit_limit
            );
        }
        let bound = keys[pidx].bound_credential_ids.clone();
        let (child_expires, child_duration) =
            resolve_child_schedule(duration_days, keys[pidx].expires_at);

        let next_id = keys.iter().map(|k| k.id).max().unwrap_or(0) + 1;
        let mut child = ApiKey::new(
            next_id,
            name,
            child_expires,
            None,
            Some(credit_limit),
            child_duration,
            bound,
        );
        child.parent_key_id = Some(parent_id);
        keys.push(child.clone());
        drop(keys);
        self.save()?;
        Ok(child)
    }

    /// 分销商更新自己的子卡密（name / enabled / credit_limit）。
    /// - `child_spent`：该子卡密已消耗的真实 credits（由用量追踪器提供），
    ///   新额度不得低于已消耗值。
    /// - 校验子卡密确属该分销商，且额度变化不超过可分配余额。
    pub fn update_child(
        &self,
        reseller_id: u32,
        child_id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        new_credit_limit: Option<f64>,
        child_spent: f64,
    ) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(cidx) = keys.iter().position(|k| k.id == child_id) else {
            return Ok(None);
        };
        if keys[cidx].parent_key_id != Some(reseller_id) {
            anyhow::bail!("无权操作该子卡密");
        }

        if let Some(new_limit) = new_credit_limit {
            if new_limit <= 0.0 {
                anyhow::bail!("子卡密额度必须大于 0");
            }
            if new_limit + ALLOC_EPS < child_spent {
                anyhow::bail!(
                    "新额度 {:.2} 不能低于已消耗 {:.2} credits",
                    new_limit,
                    child_spent
                );
            }
            let old_limit = keys[cidx].credit_limit.unwrap_or(0.0);
            let delta = new_limit - old_limit;
            if delta > 0.0 {
                let allocatable = self
                    .allocatable_credits_locked(&keys, reseller_id)
                    .ok_or_else(|| anyhow::anyhow!("分销卡密无效"))?;
                if delta > allocatable + ALLOC_EPS {
                    anyhow::bail!(
                        "超出可分配额度：可分配 {:.2}，需新增 {:.2} credits",
                        allocatable,
                        delta
                    );
                }
            }
            keys[cidx].credit_limit = Some(new_limit);
        }
        if let Some(n) = name {
            keys[cidx].name = n;
        }
        if let Some(e) = enabled {
            keys[cidx].enabled = e;
        }
        let updated = keys[cidx].clone();
        drop(keys);
        self.save()?;
        Ok(Some(updated))
    }

    /// 分销商给子卡密续费（叠加额度 / 时长）。
    /// - add_credits 受可分配余额约束。
    /// - add_days 对固定到期子卡密延长且不超过父卡密到期时间；对懒激活子卡密叠加时长。
    pub fn topup_child(
        &self,
        reseller_id: u32,
        child_id: u32,
        add_credits: Option<f64>,
        add_days: Option<f64>,
    ) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(cidx) = keys.iter().position(|k| k.id == child_id) else {
            return Ok(None);
        };
        if keys[cidx].parent_key_id != Some(reseller_id) {
            anyhow::bail!("无权操作该子卡密");
        }

        if let Some(c) = add_credits {
            if c <= 0.0 {
                anyhow::bail!("充值额度必须大于 0");
            }
            let allocatable = self
                .allocatable_credits_locked(&keys, reseller_id)
                .ok_or_else(|| anyhow::anyhow!("分销卡密无效"))?;
            if c > allocatable + ALLOC_EPS {
                anyhow::bail!(
                    "超出可分配额度：可分配 {:.2}，请求充值 {:.2} credits",
                    allocatable,
                    c
                );
            }
            keys[cidx].credit_limit = Some(keys[cidx].credit_limit.unwrap_or(0.0) + c);
        }

        if let Some(d) = add_days {
            if d <= 0.0 {
                anyhow::bail!("充值时长必须大于 0");
            }
            let parent_expires = keys.iter().find(|k| k.id == reseller_id).and_then(|p| p.expires_at);
            let child = &mut keys[cidx];
            if child.is_active() && child.expires_at.is_some() {
                let extension = chrono::Duration::milliseconds((d * 86_400_000.0) as i64);
                let mut new_expires = child.expires_at.unwrap() + extension;
                if let Some(pexp) = parent_expires {
                    new_expires = new_expires.min(pexp);
                }
                child.expires_at = Some(new_expires);
                if let Some(activated) = child.activated_at {
                    let total_ms = (new_expires - activated).num_milliseconds();
                    child.duration_days = Some(total_ms as f64 / 86_400_000.0);
                }
            } else if let Some(pexp) = parent_expires {
                // 父卡密固定到期：子卡密用固定到期并封顶到父卡密
                let now = Utc::now();
                let want = now + chrono::Duration::milliseconds((d * 86_400_000.0) as i64);
                child.expires_at = Some(want.min(pexp));
                child.duration_days = None;
                child.activated_at = None;
            } else {
                // 父卡密无期限：子卡密叠加懒激活时长
                child.duration_days = Some(child.duration_days.unwrap_or(0.0) + d);
                child.activated_at = None;
                child.expires_at = None;
            }
        }

        let updated = keys[cidx].clone();
        drop(keys);
        self.save()?;
        Ok(Some(updated))
    }

    /// 删除子卡密并把其已消耗额度结算到父卡密（钱花了不退，未用完的额度释放）。
    /// `spent` 为该子卡密真实消耗的 credits。校验归属（若提供 reseller_id）。
    pub fn delete_child_committing(
        &self,
        child_id: u32,
        spent: f64,
        expect_parent: Option<u32>,
    ) -> anyhow::Result<bool> {
        let mut keys = self.keys.write();
        let Some(cidx) = keys.iter().position(|k| k.id == child_id) else {
            return Ok(false);
        };
        let parent_id = keys[cidx].parent_key_id;
        if let Some(expected) = expect_parent {
            if parent_id != Some(expected) {
                anyhow::bail!("无权操作该子卡密");
            }
        }
        keys.remove(cidx);
        if let Some(pid) = parent_id {
            if let Some(parent) = keys.iter_mut().find(|k| k.id == pid) {
                parent.committed_credits += spent.max(0.0);
            }
        }
        drop(keys);
        self.save()?;
        Ok(true)
    }

    /// 级联删除分销卡密及其所有子卡密（父卡密消失，无需结算）。
    pub fn delete_reseller_cascade(&self, reseller_id: u32) -> anyhow::Result<bool> {
        let mut keys = self.keys.write();
        let before = keys.len();
        keys.retain(|k| k.id != reseller_id && k.parent_key_id != Some(reseller_id));
        let removed = keys.len() < before;
        drop(keys);
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// 级联启用 / 禁用某分销卡密名下的所有子卡密。
    pub fn set_children_disabled(&self, parent_id: u32, disabled: bool) -> anyhow::Result<()> {
        let mut keys = self.keys.write();
        let mut changed = false;
        for k in keys.iter_mut() {
            if k.parent_key_id == Some(parent_id) && k.enabled == disabled {
                k.enabled = !disabled;
                changed = true;
            }
        }
        drop(keys);
        if changed {
            self.save()?;
        }
        Ok(())
    }

    /// 在已持有读/写锁的情况下计算可分配余额（内部辅助）。
    fn allocatable_credits_locked(&self, keys: &[ApiKey], reseller_id: u32) -> Option<f64> {
        let reseller = keys.iter().find(|k| k.id == reseller_id)?;
        if !reseller.is_reseller {
            return None;
        }
        let budget = reseller.credit_limit?;
        let allocated: f64 = keys
            .iter()
            .filter(|k| k.parent_key_id == Some(reseller_id))
            .map(|k| k.credit_limit.unwrap_or(0.0))
            .sum();
        Some(budget - allocated - reseller.committed_credits)
    }
    // APPEND_MARKER2
}

/// 允许的浮点误差（credits 比较）
const ALLOC_EPS: f64 = 1e-6;

/// 计算子卡密的到期计划。
///
/// - 父卡密有固定到期时间：子卡密采用固定到期（立即计时），封顶到父卡密到期时间。
///   若未指定时长，则直接继承父卡密到期时间。
/// - 父卡密无到期时间：按请求采用懒激活时长（首次使用才计时），或永不过期。
fn resolve_child_schedule(
    requested_duration_days: Option<f64>,
    parent_expires_at: Option<DateTime<Utc>>,
) -> (Option<DateTime<Utc>>, Option<f64>) {
    match parent_expires_at {
        Some(parent_exp) => {
            let now = Utc::now();
            let child_exp = match requested_duration_days {
                Some(d) => {
                    let want =
                        now + chrono::Duration::milliseconds((d * 86_400_000.0) as i64);
                    want.min(parent_exp)
                }
                None => parent_exp,
            };
            (Some(child_exp), None)
        }
        None => match requested_duration_days {
            Some(d) => (None, Some(d)),
            None => (None, None),
        },
    }
}

#[cfg(test)]
mod reseller_tests {
    use super::*;

    fn new_manager() -> ApiKeyManager {
        let dir = std::env::temp_dir().join(format!("k2cc_test_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        ApiKeyManager::load(dir.join("keys.json")).unwrap()
    }

    /// 创建一个带预算的分销卡密，返回其 id
    fn make_reseller(mgr: &ApiKeyManager, budget: f64) -> u32 {
        let k = mgr
            .create("reseller".into(), None, None, Some(budget), None, None)
            .unwrap();
        mgr.set_reseller(k.id, true).unwrap();
        k.id
    }

    #[test]
    fn test_set_reseller_requires_budget() {
        let mgr = new_manager();
        let k = mgr
            .create("no-budget".into(), None, None, None, None, None)
            .unwrap();
        // 无预算不能设为分销卡密
        assert!(mgr.set_reseller(k.id, true).is_err());
    }

    #[test]
    fn test_child_cannot_be_reseller() {
        let mgr = new_manager();
        let rid = make_reseller(&mgr, 1000.0);
        let child = mgr.create_child(rid, "c1".into(), 100.0, None).unwrap();
        assert!(mgr.set_reseller(child.id, true).is_err());
    }

    #[test]
    fn test_allocatable_and_overallocation_guard() {
        let mgr = new_manager();
        let rid = make_reseller(&mgr, 1000.0);
        assert_eq!(mgr.allocatable_credits(rid), Some(1000.0));

        mgr.create_child(rid, "c1".into(), 600.0, None).unwrap();
        assert_eq!(mgr.allocatable_credits(rid), Some(400.0));

        // 超额分配被拒绝
        assert!(mgr.create_child(rid, "c2".into(), 500.0, None).is_err());
        // 刚好在余额内可以
        let c2 = mgr.create_child(rid, "c2".into(), 400.0, None).unwrap();
        assert_eq!(mgr.allocatable_credits(rid), Some(0.0));
        assert_eq!(c2.parent_key_id, Some(rid));
    }

    #[test]
    fn test_child_inherits_binding() {
        let mgr = new_manager();
        // reseller 绑定账号 [4]
        let k = mgr
            .create("r".into(), None, None, Some(500.0), None, Some(vec![4]))
            .unwrap();
        mgr.set_reseller(k.id, true).unwrap();
        let child = mgr.create_child(k.id, "c".into(), 100.0, None).unwrap();
        assert_eq!(child.bound_credential_ids, Some(vec![4]));
    }

    #[test]
    fn test_delete_child_commits_spent_and_frees_remainder() {
        let mgr = new_manager();
        let rid = make_reseller(&mgr, 1000.0);
        let child = mgr.create_child(rid, "c1".into(), 600.0, None).unwrap();
        assert_eq!(mgr.allocatable_credits(rid), Some(400.0));

        // 子卡密消耗了 250 credits，删除时结算
        mgr.delete_child_committing(child.id, 250.0, Some(rid))
            .unwrap();
        // 已分配的 600 释放，但 250 计入 committed => 可分配 1000 - 0 - 250 = 750
        assert_eq!(mgr.allocatable_credits(rid), Some(750.0));
        assert_eq!(mgr.get(rid).unwrap().committed_credits, 250.0);
    }

    #[test]
    fn test_update_child_ownership_and_spent_floor() {
        let mgr = new_manager();
        let rid1 = make_reseller(&mgr, 1000.0);
        let rid2 = make_reseller(&mgr, 1000.0);
        let child = mgr.create_child(rid1, "c1".into(), 500.0, None).unwrap();

        // 另一个分销商无权修改
        assert!(
            mgr.update_child(rid2, child.id, None, None, Some(300.0), 0.0)
                .is_err()
        );
        // 新额度不能低于已消耗
        assert!(
            mgr.update_child(rid1, child.id, None, None, Some(100.0), 200.0)
                .is_err()
        );
        // 合法下调
        let updated = mgr
            .update_child(rid1, child.id, None, None, Some(300.0), 100.0)
            .unwrap()
            .unwrap();
        assert_eq!(updated.credit_limit, Some(300.0));
        assert_eq!(mgr.allocatable_credits(rid1), Some(700.0));
    }

    #[test]
    fn test_update_child_increase_capped_by_allocatable() {
        let mgr = new_manager();
        let rid = make_reseller(&mgr, 1000.0);
        let child = mgr.create_child(rid, "c1".into(), 600.0, None).unwrap();
        // 已分配 600，可分配 400；把 child 上调到 1200 需要新增 600 > 400 => 拒绝
        assert!(
            mgr.update_child(rid, child.id, None, None, Some(1200.0), 0.0)
                .is_err()
        );
        // 上调到 1000 需要新增 400，正好可以
        assert!(
            mgr.update_child(rid, child.id, None, None, Some(1000.0), 0.0)
                .is_ok()
        );
    }

    #[test]
    fn test_topup_child_credits_capped() {
        let mgr = new_manager();
        let rid = make_reseller(&mgr, 1000.0);
        let child = mgr.create_child(rid, "c1".into(), 600.0, None).unwrap();
        // 可分配 400，充 500 被拒
        assert!(
            mgr.topup_child(rid, child.id, Some(500.0), None)
                .is_err()
        );
        // 充 400 可以
        let updated = mgr
            .topup_child(rid, child.id, Some(400.0), None)
            .unwrap()
            .unwrap();
        assert_eq!(updated.credit_limit, Some(1000.0));
        assert_eq!(mgr.allocatable_credits(rid), Some(0.0));
    }

    #[test]
    fn test_child_expiry_capped_to_parent() {
        let mgr = new_manager();
        // 父卡密 10 天后到期
        let parent_exp = Utc::now() + chrono::Duration::days(10);
        let k = mgr
            .create("r".into(), Some(parent_exp), None, Some(1000.0), None, None)
            .unwrap();
        mgr.set_reseller(k.id, true).unwrap();
        // 子卡密要 100 天，应被封顶到父卡密到期时间
        let child = mgr
            .create_child(k.id, "c".into(), 100.0, Some(100.0))
            .unwrap();
        let child_exp = child.expires_at.unwrap();
        assert!(child_exp <= parent_exp);
        // 允许 1 秒误差
        assert!((child_exp - parent_exp).num_seconds().abs() <= 1);
    }

    #[test]
    fn test_delete_reseller_cascade() {
        let mgr = new_manager();
        let rid = make_reseller(&mgr, 1000.0);
        mgr.create_child(rid, "c1".into(), 100.0, None).unwrap();
        mgr.create_child(rid, "c2".into(), 100.0, None).unwrap();
        assert_eq!(mgr.list_children(rid).len(), 2);
        mgr.delete_reseller_cascade(rid).unwrap();
        assert_eq!(mgr.list_children(rid).len(), 0);
        assert!(mgr.get(rid).is_none());
    }

    #[test]
    fn test_cascade_disable_children() {
        let mgr = new_manager();
        let rid = make_reseller(&mgr, 1000.0);
        let c1 = mgr.create_child(rid, "c1".into(), 100.0, None).unwrap();
        mgr.set_children_disabled(rid, true).unwrap();
        assert!(!mgr.get(c1.id).unwrap().enabled);
        mgr.set_children_disabled(rid, false).unwrap();
        assert!(mgr.get(c1.id).unwrap().enabled);
    }
}
