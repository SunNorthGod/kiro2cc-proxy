// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin API Key 管理处理器

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;

use super::{
    middleware::AdminState,
    types::{
        AdminErrorResponse, CreateApiKeyRequest, SuccessResponse, TopUpApiKeyRequest,
        UpdateApiKeyRequest,
    },
};

/// GET /api/admin/server-info
/// 获取服务器连接信息（主 API Key）
pub async fn get_server_info(State(state): State<AdminState>) -> impl IntoResponse {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ServerInfo {
        master_api_key: Option<String>,
        version: String,
    }
    Json(ServerInfo {
        master_api_key: state.master_api_key.as_ref().map(|k| k.read().clone()),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// GET /api/admin/api-keys
/// 列出所有 API Key
pub async fn list_api_keys(State(state): State<AdminState>) -> impl IntoResponse {
    match &state.api_key_manager {
        Some(manager) => Json(manager.list()).into_response(),
        None => {
            let error = AdminErrorResponse::internal_error("API Key 管理未启用");
            (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response()
        }
    }
}

/// POST /api/admin/api-keys
/// 创建新 API Key
pub async fn create_api_key(
    State(state): State<AdminState>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.create(
        payload.name,
        payload.expires_at,
        None, // 美元额度已废弃，全链路只用 credits
        payload.credit_limit,
        payload.duration_days,
        payload.bound_credential_ids,
    ) {
        Ok(api_key) => {
            // 开卡即记一笔充值流水（仅额度卡/时长卡；纯永久卡无进账不记）
            if let Some(rt) = &state.recharge_tracker
                && (api_key.credit_limit.is_some() || api_key.duration_days.is_some())
            {
                rt.record(
                    api_key.id,
                    "create",
                    api_key.credit_limit,
                    api_key.duration_days,
                    api_key.credit_limit,
                    api_key.expires_at,
                    "admin",
                    None,
                );
            }
            (StatusCode::CREATED, Json(api_key)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// PUT /api/admin/api-keys/:id
/// 更新 API Key
pub async fn update_api_key(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    Json(payload): Json<UpdateApiKeyRequest>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.update(
        id,
        payload.name,
        payload.enabled,
        payload.expires_at,
        None, // 美元额度已废弃，全链路只用 credits
        payload.credit_limit,
        payload.duration_days,
        payload.bound_credential_ids,
    ) {
        Ok(Some(api_key)) => {
            // 有子卡密的父卡密启用/禁用时级联到其子卡密
            if let Some(enabled) = payload.enabled
                && manager.has_children(id)
            {
                if let Err(e) = manager.set_children_disabled(id, !enabled) {
                    tracing::warn!(parent_id = id, error = %e, "级联设置子卡密启用状态失败");
                }
            }
            Json(api_key).into_response()
        }
        Ok(None) => {
            let error = AdminErrorResponse::not_found(format!("API Key #{} 不存在", id));
            (StatusCode::NOT_FOUND, Json(error)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// POST /api/admin/api-keys/:id/topup
/// 给 Key 续费/充值：叠加时长或额度（增量）
pub async fn top_up_api_key(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    Json(payload): Json<TopUpApiKeyRequest>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    if payload.add_days.is_none() && payload.add_credits.is_none() {
        let error = AdminErrorResponse::invalid_request("请至少提供 addDays 或 addCredits");
        return (StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    match manager.topup(id, payload.add_days, payload.add_credits) {
        Ok(Some(api_key)) => {
            if let Some(rt) = &state.recharge_tracker {
                rt.record(
                    api_key.id,
                    "topup",
                    payload.add_credits,
                    payload.add_days,
                    api_key.credit_limit,
                    api_key.expires_at,
                    "admin",
                    None,
                );
            }
            Json(api_key).into_response()
        }
        Ok(None) => {
            let error = AdminErrorResponse::not_found(format!("API Key #{} 不存在", id));
            (StatusCode::NOT_FOUND, Json(error)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// GET /api/admin/api-keys/:id/recharges?page=1&page_size=50
/// 分页获取单个 API Key 的充值/开卡流水
pub async fn get_key_recharge_records(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.recharge_tracker else {
        let error = AdminErrorResponse::internal_error("充值流水未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    Json(tracker.get_records_paged(id, page, page_size)).into_response()
}

/// DELETE /api/admin/api-keys/:id
/// 删除 API Key（有子卡密的父卡密会级联删除其所有子卡密）
pub async fn delete_api_key(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    // 有子卡密的父卡密：级联删除其名下所有子卡密
    let result = if manager.has_children(id) {
        manager.delete_with_children(id)
    } else {
        manager.delete(id)
    };

    match result {
        Ok(true) => Json(SuccessResponse::new(format!("API Key #{} 已删除", id))).into_response(),
        Ok(false) => {
            let error = AdminErrorResponse::not_found(format!("API Key #{} 不存在", id));
            (StatusCode::NOT_FOUND, Json(error)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// GET /api/admin/api-keys/usage
/// 获取所有 API Key 的用量概览
pub async fn get_all_usage(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_all_summaries()).into_response()
}

/// GET /api/admin/api-keys/:id/usage
/// 获取单个 API Key 的用量汇总
pub async fn get_key_usage(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_summary(id)).into_response()
}

/// DELETE /api/admin/api-keys/:id/usage
/// 重置单个 API Key 的用量记录
pub async fn reset_key_usage(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    match tracker.reset(id) {
        Ok(()) => Json(SuccessResponse::new(format!("API Key #{} 用量已重置", id))).into_response(),
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// GET /api/admin/api-keys/:id/usage/records?page=1&page_size=50
/// 分页获取单个 API Key 的原始请求记录
pub async fn get_key_usage_records(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    let labels = state.service.credential_labels();
    Json(tracker.get_records_paged(id, page, page_size, &labels)).into_response()
}

/// GET /api/admin/credentials/:id/usage/records?page=1&page_size=50
/// 分页获取单个账号的原始请求记录
pub async fn get_credential_usage_records(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    let labels = state.service.credential_labels();
    Json(tracker.get_records_paged_by_credential(id, page, page_size, &labels)).into_response()
}

/// GET /api/admin/credentials/:id/usage/today
/// 获取单个账号在 CST 今日的用量汇总
pub async fn get_credential_today_summary(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_today_summary_for_credential(id)).into_response()
}

/// GET /api/admin/rpm
/// 获取实时 RPM 数据（含 sticky cache 命中/未命中统计）
pub async fn get_rpm(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(rpm_tracker) = &state.rpm_tracker else {
        let error = AdminErrorResponse::internal_error("RPM 监控未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let (sticky_hits, sticky_misses) = state.service.sticky_metrics();
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RpmAndCacheSnapshot {
        #[serde(flatten)]
        rpm: crate::model::rpm::RpmSnapshot,
        sticky_hits: u64,
        sticky_misses: u64,
    }
    Json(RpmAndCacheSnapshot {
        rpm: rpm_tracker.snapshot(),
        sticky_hits,
        sticky_misses,
    })
    .into_response()
}

/// GET /api/admin/usage/daily
/// 获取所有日期的用量汇总（按日期降序）
pub async fn get_daily_usage(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_daily_summaries()).into_response()
}

/// GET /api/admin/overview
/// 首页概览：全历史总量 + 近 30 天序列 + 模型分布 + Top API Key（供首页一次拉取）
pub async fn get_overview(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_overview(30)).into_response()
}

/// GET /api/admin/usage/daily/{date}/records?page=1&page_size=50
/// 分页获取指定日期的原始请求记录（最多 2000 条）
pub async fn get_daily_usage_records(
    State(state): State<AdminState>,
    Path(date): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50);
    let labels = state.service.credential_labels();
    Json(tracker.get_records_paged_by_date(&date, page, page_size, &labels)).into_response()
}

/// GET /api/admin/credentials/:id/failure-logs?page=1&page_size=50
/// 分页获取指定账号的失败日志
pub async fn get_failure_logs(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(store) = &state.failure_log_store else {
        let error = AdminErrorResponse::internal_error("失败日志未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    Json(store.get_paged(id, page, page_size)).into_response()
}

/// GET /api/admin/credentials/:id/throttle-logs?page=1&page_size=50
/// 分页获取指定账号的限流日志
pub async fn get_throttle_logs(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(store) = &state.throttle_log_store else {
        let error = AdminErrorResponse::internal_error("限流日志未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    Json(store.get_paged(id, page, page_size)).into_response()
}
