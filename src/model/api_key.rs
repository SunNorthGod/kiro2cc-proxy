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
    /// 父卡密 ID（仅子卡密有值）。子卡密从父卡密的共享额度池中预扣额度。
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_key_id: Option<u32>,
    /// 已结算额度（仅有子卡密的父卡密有值）。当子卡密被删除时，把它已消耗的真实
    /// credits 累加到父卡密的 committed_credits，从而"用掉的钱不退回"，未用完的额度释放。
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
            parent_key_id: None,
            committed_credits: 0.0,
        }
    }

    /// 是否可以开设 / 管理子卡密。
    /// 规则：必须是"按额度"的卡（有 credit_limit 预算），且自身不是别人的子卡密（单层）。
    pub fn can_manage_subkeys(&self) -> bool {
        self.credit_limit.is_some() && self.parent_key_id.is_none()
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
        /// 已预留额度（Σ子卡密额度 + 已结算）。父卡密自己可用额度 = credit_limit - reserved_credits。
        reserved_credits: f64,
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
                    let reserved = reserved_credits_for(&keys, api_key);
                    ApiKeyAuthResult::Valid {
                        id: api_key.id,
                        name: api_key.name.clone(),
                        spending_limit: api_key.spending_limit,
                        credit_limit: api_key.credit_limit,
                        bound_credential_ids: api_key.bound_credential_ids.clone(),
                        reserved_credits: reserved,
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
            Some(api_key) => {
                let reserved = reserved_credits_for(&keys, api_key);
                ApiKeyAuthResult::Valid {
                    id: api_key.id,
                    name: api_key.name.clone(),
                    spending_limit: api_key.spending_limit,
                    credit_limit: api_key.credit_limit,
                    bound_credential_ids: api_key.bound_credential_ids.clone(),
                    reserved_credits: reserved,
                }
            }
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

    // ==================== 子卡密（共享额度池）相关 ====================

    /// 获取指定 key 的克隆
    pub fn get(&self, id: u32) -> Option<ApiKey> {
        self.keys.read().iter().find(|k| k.id == id).cloned()
    }

    /// 列出某父卡密的所有子卡密
    pub fn list_children(&self, parent_id: u32) -> Vec<ApiKey> {
        self.keys
            .read()
            .iter()
            .filter(|k| k.parent_key_id == Some(parent_id))
            .cloned()
            .collect()
    }

    /// 计算父卡密还能再分配给新子卡密的额度：
    ///   预算 - 父卡密自己已花(own_spent) - Σ(子卡密已分配额度) - 已结算额度
    ///
    /// 这是"共享额度池"模型的核心：父卡密自己消耗、子卡密占用、已删除子卡密的结算,
    /// 三者共同占用同一个 credit_limit 预算，任何分配都不能让总占用超过预算。
    /// 返回 None 表示该 key 不能管理子卡密（不存在 / 无额度预算 / 本身是子卡密）。
    pub fn allocatable_credits(&self, parent_id: u32, own_spent: f64) -> Option<f64> {
        let keys = self.keys.read();
        let parent = keys.iter().find(|k| k.id == parent_id)?;
        if !parent.can_manage_subkeys() {
            return None;
        }
        Some(allocatable_locked(&keys, parent, own_spent))
    }

    /// 父卡密开子卡密。原子校验共享池可分配额度，继承父卡密的账号绑定，
    /// 到期时间受父卡密约束（见 resolve_child_schedule）。
    /// `parent_own_spent`：父卡密自己已消耗的真实 credits（由用量追踪器提供）。
    pub fn create_child(
        &self,
        parent_id: u32,
        parent_own_spent: f64,
        name: String,
        credit_limit: f64,
        duration_days: Option<f64>,
    ) -> anyhow::Result<ApiKey> {
        if credit_limit <= 0.0 {
            anyhow::bail!("子卡密额度必须大于 0");
        }
        let mut keys = self.keys.write();
        let Some(pidx) = keys.iter().position(|k| k.id == parent_id) else {
            anyhow::bail!("卡密不存在");
        };
        if !keys[pidx].can_manage_subkeys() {
            anyhow::bail!("该卡密不支持开子卡密（需为按额度的卡，且本身不是子卡密）");
        }
        let allocatable = allocatable_locked(&keys, &keys[pidx], parent_own_spent);
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

    /// 父卡密更新自己的子卡密（name / enabled / credit_limit）。
    /// - `child_spent`：该子卡密已消耗的真实 credits，新额度不得低于已消耗值。
    /// - `parent_own_spent`：父卡密自己已消耗的真实 credits（用于共享池校验）。
    /// - 校验子卡密确属该父卡密，且额度增量不超过共享池可分配余额。
    #[allow(clippy::too_many_arguments)]
    pub fn update_child(
        &self,
        parent_id: u32,
        parent_own_spent: f64,
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
        if keys[cidx].parent_key_id != Some(parent_id) {
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
                let parent = keys
                    .iter()
                    .find(|k| k.id == parent_id)
                    .ok_or_else(|| anyhow::anyhow!("父卡密不存在"))?;
                let allocatable = allocatable_locked(&keys, parent, parent_own_spent);
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

    /// 父卡密给子卡密续费（叠加额度 / 时长）。
    /// - add_credits 受共享池可分配余额约束。
    /// - add_days 对固定到期子卡密延长且不超过父卡密到期时间；对懒激活子卡密叠加时长。
    #[allow(clippy::too_many_arguments)]
    pub fn topup_child(
        &self,
        parent_id: u32,
        parent_own_spent: f64,
        child_id: u32,
        add_credits: Option<f64>,
        add_days: Option<f64>,
    ) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(cidx) = keys.iter().position(|k| k.id == child_id) else {
            return Ok(None);
        };
        if keys[cidx].parent_key_id != Some(parent_id) {
            anyhow::bail!("无权操作该子卡密");
        }

        if let Some(c) = add_credits {
            if c <= 0.0 {
                anyhow::bail!("充值额度必须大于 0");
            }
            let parent = keys
                .iter()
                .find(|k| k.id == parent_id)
                .ok_or_else(|| anyhow::anyhow!("父卡密不存在"))?;
            let allocatable = allocatable_locked(&keys, parent, parent_own_spent);
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
            let parent_expires = keys.iter().find(|k| k.id == parent_id).and_then(|p| p.expires_at);
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
    /// `spent` 为该子卡密真实消耗的 credits。校验归属（若提供 expect_parent）。
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

    /// 级联删除父卡密及其所有子卡密（父卡密消失，无需结算）。
    pub fn delete_with_children(&self, parent_id: u32) -> anyhow::Result<bool> {
        let mut keys = self.keys.write();
        let before = keys.len();
        keys.retain(|k| k.id != parent_id && k.parent_key_id != Some(parent_id));
        let removed = keys.len() < before;
        drop(keys);
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// 判断某 key 是否有子卡密
    pub fn has_children(&self, parent_id: u32) -> bool {
        self.keys
            .read()
            .iter()
            .any(|k| k.parent_key_id == Some(parent_id))
    }

    /// 级联启用 / 禁用某父卡密名下的所有子卡密。
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
    // APPEND_MARKER2
}

/// 允许的浮点误差（credits 比较）
const ALLOC_EPS: f64 = 1e-6;

/// 在已持有锁的情况下，计算某父卡密的"已预留额度"：
///   Σ(子卡密已分配额度) + 已结算额度(committed_credits)
///
/// 父卡密自己可用额度 = credit_limit - reserved；无 credit_limit（不限额）时预留视为 0。
fn reserved_credits_for(keys: &[ApiKey], parent: &ApiKey) -> f64 {
    let allocated: f64 = keys
        .iter()
        .filter(|k| k.parent_key_id == Some(parent.id))
        .map(|k| k.credit_limit.unwrap_or(0.0))
        .sum();
    allocated + parent.committed_credits
}

/// 在已持有锁的情况下计算共享池可分配余额：
///   预算 - 父卡密自己已花 - 已预留(Σ子卡密 + 已结算)
fn allocatable_locked(keys: &[ApiKey], parent: &ApiKey, own_spent: f64) -> f64 {
    let budget = parent.credit_limit.unwrap_or(0.0);
    budget - own_spent - reserved_credits_for(keys, parent)
}

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
mod subkey_tests {
    use super::*;

    fn new_manager() -> ApiKeyManager {
        let dir = std::env::temp_dir().join(format!("k2cc_test_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        ApiKeyManager::load(dir.join("keys.json")).unwrap()
    }

    /// 创建一个带额度预算的父卡密，返回其 id
    fn make_parent(mgr: &ApiKeyManager, budget: f64) -> u32 {
        mgr.create("parent".into(), None, None, Some(budget), None, None)
            .unwrap()
            .id
    }

    #[test]
    fn test_can_manage_subkeys() {
        let mgr = new_manager();
        // 按额度的卡：可以
        let p = mgr.get(make_parent(&mgr, 1000.0)).unwrap();
        assert!(p.can_manage_subkeys());
        // 无额度（时长/永久）的卡：不行
        let t = mgr
            .create("t".into(), None, None, None, Some(30.0), None)
            .unwrap();
        assert!(!t.can_manage_subkeys());
        // 子卡密：不行（单层）
        let child = mgr.create_child(p.id, 0.0, "c".into(), 100.0, None).unwrap();
        assert!(!child.can_manage_subkeys());
    }

    #[test]
    fn test_shared_pool_own_spend_reduces_allocatable() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        // 无消耗、无子卡：可分配 1000
        assert_eq!(mgr.allocatable_credits(pid, 0.0), Some(1000.0));
        // 父卡自己已花 300：可分配 700
        assert_eq!(mgr.allocatable_credits(pid, 300.0), Some(700.0));
        // 开一个 400 的子卡（父已花 300）：需 400 <= 700 OK
        mgr.create_child(pid, 300.0, "c1".into(), 400.0, None).unwrap();
        // 现在可分配 = 1000 - 300(自花) - 400(子卡) = 300
        assert_eq!(mgr.allocatable_credits(pid, 300.0), Some(300.0));
    }

    #[test]
    fn test_overallocation_guard_shared_pool() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        // 父卡自己已花 600，剩 400 可分
        assert!(mgr.create_child(pid, 600.0, "c1".into(), 500.0, None).is_err());
        // 分 400 正好
        assert!(mgr.create_child(pid, 600.0, "c1".into(), 400.0, None).is_ok());
        // 池子已满：再分任何额度都失败
        assert!(mgr.create_child(pid, 600.0, "c2".into(), 1.0, None).is_err());
    }

    #[test]
    fn test_child_inherits_binding() {
        let mgr = new_manager();
        let k = mgr
            .create("p".into(), None, None, Some(500.0), None, Some(vec![4]))
            .unwrap();
        let child = mgr.create_child(k.id, 0.0, "c".into(), 100.0, None).unwrap();
        assert_eq!(child.bound_credential_ids, Some(vec![4]));
    }

    #[test]
    fn test_delete_child_commits_spent_and_frees_remainder() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        let child = mgr.create_child(pid, 0.0, "c1".into(), 600.0, None).unwrap();
        assert_eq!(mgr.allocatable_credits(pid, 0.0), Some(400.0));

        // 子卡消耗 250，删除时结算
        mgr.delete_child_committing(child.id, 250.0, Some(pid)).unwrap();
        // 600 预留释放，250 计入 committed => 可分配 1000 - 0 - 250 = 750
        assert_eq!(mgr.allocatable_credits(pid, 0.0), Some(750.0));
        assert_eq!(mgr.get(pid).unwrap().committed_credits, 250.0);
    }

    #[test]
    fn test_parent_effective_limit_via_reserved() {
        // 验证 authenticate 返回的 reserved_credits 正确（父卡自己可用 = limit - reserved）
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        let parent = mgr.get(pid).unwrap();
        // 开 300 子卡 + 删一个花了 100 的子卡
        mgr.create_child(pid, 0.0, "c1".into(), 300.0, None).unwrap();
        let c2 = mgr.create_child(pid, 0.0, "c2".into(), 200.0, None).unwrap();
        mgr.delete_child_committing(c2.id, 100.0, Some(pid)).unwrap();
        // reserved = 300(存活子卡) + 100(committed) = 400 => 父卡自己可用到 600
        let keys = mgr.keys.read();
        let p = keys.iter().find(|k| k.id == pid).unwrap();
        assert_eq!(reserved_credits_for(&keys, p), 400.0);
        drop(keys);
        let _ = parent;
    }

    #[test]
    fn test_update_child_ownership_and_spent_floor() {
        let mgr = new_manager();
        let pid1 = make_parent(&mgr, 1000.0);
        let pid2 = make_parent(&mgr, 1000.0);
        let child = mgr.create_child(pid1, 0.0, "c1".into(), 500.0, None).unwrap();

        // 另一个父卡无权修改
        assert!(
            mgr.update_child(pid2, 0.0, child.id, None, None, Some(300.0), 0.0)
                .is_err()
        );
        // 新额度不能低于已消耗
        assert!(
            mgr.update_child(pid1, 0.0, child.id, None, None, Some(100.0), 200.0)
                .is_err()
        );
        // 合法下调
        let updated = mgr
            .update_child(pid1, 0.0, child.id, None, None, Some(300.0), 100.0)
            .unwrap()
            .unwrap();
        assert_eq!(updated.credit_limit, Some(300.0));
        assert_eq!(mgr.allocatable_credits(pid1, 0.0), Some(700.0));
    }

    #[test]
    fn test_update_child_increase_capped_by_shared_pool() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        let child = mgr.create_child(pid, 0.0, "c1".into(), 600.0, None).unwrap();
        // 父卡自己已花 100；已分配 600 => 可分配 300。上调到 1000 需新增 400 > 300 拒绝
        assert!(
            mgr.update_child(pid, 100.0, child.id, None, None, Some(1000.0), 0.0)
                .is_err()
        );
        // 上调到 900 需新增 300，正好
        assert!(
            mgr.update_child(pid, 100.0, child.id, None, None, Some(900.0), 0.0)
                .is_ok()
        );
    }

    #[test]
    fn test_topup_child_credits_capped_shared_pool() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        let child = mgr.create_child(pid, 0.0, "c1".into(), 600.0, None).unwrap();
        // 父卡自花 200 => 可分配 200。充 300 被拒
        assert!(mgr.topup_child(pid, 200.0, child.id, Some(300.0), None).is_err());
        // 充 200 可以
        let updated = mgr
            .topup_child(pid, 200.0, child.id, Some(200.0), None)
            .unwrap()
            .unwrap();
        assert_eq!(updated.credit_limit, Some(800.0));
    }

    #[test]
    fn test_child_expiry_capped_to_parent() {
        let mgr = new_manager();
        let parent_exp = Utc::now() + chrono::Duration::days(10);
        let k = mgr
            .create("p".into(), Some(parent_exp), None, Some(1000.0), None, None)
            .unwrap();
        let child = mgr.create_child(k.id, 0.0, "c".into(), 100.0, Some(100.0)).unwrap();
        let child_exp = child.expires_at.unwrap();
        assert!(child_exp <= parent_exp);
        assert!((child_exp - parent_exp).num_seconds().abs() <= 1);
    }

    #[test]
    fn test_delete_with_children_cascade() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        mgr.create_child(pid, 0.0, "c1".into(), 100.0, None).unwrap();
        mgr.create_child(pid, 0.0, "c2".into(), 100.0, None).unwrap();
        assert_eq!(mgr.list_children(pid).len(), 2);
        mgr.delete_with_children(pid).unwrap();
        assert_eq!(mgr.list_children(pid).len(), 0);
        assert!(mgr.get(pid).is_none());
    }

    #[test]
    fn test_cascade_disable_children() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        let c1 = mgr.create_child(pid, 0.0, "c1".into(), 100.0, None).unwrap();
        mgr.set_children_disabled(pid, true).unwrap();
        assert!(!mgr.get(c1.id).unwrap().enabled);
        mgr.set_children_disabled(pid, false).unwrap();
        assert!(mgr.get(c1.id).unwrap().enabled);
    }

    #[test]
    fn test_child_cannot_open_subkeys() {
        let mgr = new_manager();
        let pid = make_parent(&mgr, 1000.0);
        let child = mgr.create_child(pid, 0.0, "c1".into(), 500.0, None).unwrap();
        // 子卡密不能再开子卡（单层）
        assert!(mgr.create_child(child.id, 0.0, "gc".into(), 100.0, None).is_err());
    }
}
