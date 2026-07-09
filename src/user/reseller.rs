// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 分销商（reseller）API 处理器
//!
//! 分销卡密的持有者可以在自己的预算范围内自助开设 / 管理子卡密。
//! 采用"预分配"记账：开子卡密即从分销预算中划走额度；删除子卡密时把已消耗
//! 的真实 credits 结算进父卡密（花掉的不退，未用完的释放）。

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use super::middleware::{ResellerContext, UserErrorResponse, UserState};

/// 单个子卡密的对外视图（含实时用量）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubKeyView {
    pub id: u32,
    pub key: String,
    pub name: String,
    pub enabled: bool,
    pub credit_limit: Option<f64>,
    /// 已消耗真实 credits
    pub used_credits: f64,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub duration_days: Option<f64>,
    pub activated_at: Option<String>,
}

/// 分销商概览响应
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResellerOverview {
    pub id: u32,
    pub name: String,
    /// 额度预算（credit_limit）
    pub budget: Option<f64>,
    /// 父卡密自己已消耗的真实 credits（共享额度池的一部分）
    pub own_used: f64,
    /// 已分配给存活子卡密的额度之和
    pub allocated: f64,
    /// 已结算额度（删除子卡密时累计的真实消耗）
    pub committed: f64,
    /// 当前可再分配额度
    pub allocatable: f64,
    /// 子卡密总数
    pub sub_key_count: usize,
    pub expires_at: Option<String>,
    pub sub_keys: Vec<SubKeyView>,
}

/// GET /api/user/reseller/overview
pub async fn overview(
    State(state): State<UserState>,
    Extension(ctx): Extension<ResellerContext>,
) -> impl IntoResponse {
    let Some(reseller) = state.api_key_manager.get(ctx.reseller_id) else {
        return err(StatusCode::NOT_FOUND, "分销卡密不存在");
    };
    let children = state.api_key_manager.list_children(ctx.reseller_id);
    let allocated: f64 = children
        .iter()
        .map(|c| c.credit_limit.unwrap_or(0.0))
        .sum();
    // 父卡密自己已消耗的真实 credits（共享额度池的一部分）
    let own_used = state.usage_tracker.get_total_credits(ctx.reseller_id);
    let allocatable = state
        .api_key_manager
        .allocatable_credits(ctx.reseller_id, own_used)
        .unwrap_or(0.0);

    let sub_keys: Vec<SubKeyView> = children
        .iter()
        .map(|c| SubKeyView {
            id: c.id,
            key: c.key.clone(),
            name: c.name.clone(),
            enabled: c.enabled,
            credit_limit: c.credit_limit,
            used_credits: state.usage_tracker.get_total_credits(c.id),
            created_at: c.created_at.to_rfc3339(),
            expires_at: c.expires_at.map(|t| t.to_rfc3339()),
            duration_days: c.duration_days,
            activated_at: c.activated_at.map(|t| t.to_rfc3339()),
        })
        .collect();

    let overview = ResellerOverview {
        id: reseller.id,
        name: reseller.name.clone(),
        budget: reseller.credit_limit,
        own_used,
        allocated,
        committed: reseller.committed_credits,
        allocatable,
        sub_key_count: sub_keys.len(),
        expires_at: reseller.expires_at.map(|t| t.to_rfc3339()),
        sub_keys,
    };
    (StatusCode::OK, Json(overview)).into_response()
}

/// 创建子卡密请求
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubKeyRequest {
    pub name: String,
    /// 分配的额度（credits，必填，> 0）
    pub credit_limit: f64,
    /// 有效期天数（可选；懒激活或按父卡密到期封顶）
    #[serde(default)]
    pub duration_days: Option<f64>,
}

/// POST /api/user/reseller/sub-keys
pub async fn create_sub_key(
    State(state): State<UserState>,
    Extension(ctx): Extension<ResellerContext>,
    Json(payload): Json<CreateSubKeyRequest>,
) -> impl IntoResponse {
    if payload.name.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "请填写子卡密名称");
    }
    let own_used = state.usage_tracker.get_total_credits(ctx.reseller_id);
    match state.api_key_manager.create_child(
        ctx.reseller_id,
        own_used,
        payload.name,
        payload.credit_limit,
        payload.duration_days,
    ) {
        Ok(child) => (StatusCode::CREATED, Json(child)).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

/// 更新子卡密请求
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSubKeyRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// 新额度（credits）。省略则不改。
    #[serde(default)]
    pub credit_limit: Option<f64>,
}

/// PUT /api/user/reseller/sub-keys/:id
pub async fn update_sub_key(
    State(state): State<UserState>,
    Extension(ctx): Extension<ResellerContext>,
    Path(child_id): Path<u32>,
    Json(payload): Json<UpdateSubKeyRequest>,
) -> impl IntoResponse {
    let spent = state.usage_tracker.get_total_credits(child_id);
    let own_used = state.usage_tracker.get_total_credits(ctx.reseller_id);
    match state.api_key_manager.update_child(
        ctx.reseller_id,
        own_used,
        child_id,
        payload.name,
        payload.enabled,
        payload.credit_limit,
        spent,
    ) {
        Ok(Some(child)) => (StatusCode::OK, Json(child)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "子卡密不存在"),
        Err(e) => err(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

/// 子卡密续费请求
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopUpSubKeyRequest {
    #[serde(default)]
    pub add_credits: Option<f64>,
    #[serde(default)]
    pub add_days: Option<f64>,
}

/// POST /api/user/reseller/sub-keys/:id/topup
pub async fn topup_sub_key(
    State(state): State<UserState>,
    Extension(ctx): Extension<ResellerContext>,
    Path(child_id): Path<u32>,
    Json(payload): Json<TopUpSubKeyRequest>,
) -> impl IntoResponse {
    if payload.add_credits.is_none() && payload.add_days.is_none() {
        return err(StatusCode::BAD_REQUEST, "请至少提供 addCredits 或 addDays");
    }
    let own_used = state.usage_tracker.get_total_credits(ctx.reseller_id);
    match state.api_key_manager.topup_child(
        ctx.reseller_id,
        own_used,
        child_id,
        payload.add_credits,
        payload.add_days,
    ) {
        Ok(Some(child)) => (StatusCode::OK, Json(child)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "子卡密不存在"),
        Err(e) => err(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

/// DELETE /api/user/reseller/sub-keys/:id
pub async fn delete_sub_key(
    State(state): State<UserState>,
    Extension(ctx): Extension<ResellerContext>,
    Path(child_id): Path<u32>,
) -> impl IntoResponse {
    let spent = state.usage_tracker.get_total_credits(child_id);
    match state
        .api_key_manager
        .delete_child_committing(child_id, spent, Some(ctx.reseller_id))
    {
        Ok(true) => {
            // 删除子卡密后，其用量记录已无归属，清理掉以免污染报表
            let _ = state.usage_tracker.reset(child_id);
            (
                StatusCode::OK,
                Json(SuccessBody {
                    success: true,
                    message: format!("子卡密 #{} 已删除", child_id),
                }),
            )
                .into_response()
        }
        Ok(false) => err(StatusCode::NOT_FOUND, "子卡密不存在"),
        Err(e) => err(StatusCode::FORBIDDEN, e.to_string()),
    }
}

#[derive(Serialize)]
struct SuccessBody {
    success: bool,
    message: String,
}

/// 构造错误响应
fn err(status: StatusCode, msg: impl Into<String>) -> axum::response::Response {
    (
        status,
        Json(UserErrorResponse { error: msg.into() }),
    )
        .into_response()
}
