// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 中转对接（备用路由）Admin API 处理器

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::relay::forward;
use crate::relay::types::{
    CreateRelayRequest, RelayModelsResponse, RelayView, UpdateRelayRequest,
};

use super::middleware::AdminState;
use super::types::{AdminErrorResponse, SuccessResponse};

fn no_relay() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!(AdminErrorResponse::internal_error(
            "中转对接功能未启用（需配置 Admin）"
        ))),
    )
        .into_response()
}

/// GET /api/admin/relays — 列出所有中转（脱敏）
pub async fn list_relays(State(state): State<AdminState>) -> Response {
    let Some(mgr) = state.relay_manager.as_ref() else {
        return no_relay();
    };
    let views: Vec<RelayView> = mgr
        .list()
        .iter()
        .map(|r| {
            let mut v = r.to_masked();
            if let Some(tracker) = state.usage_tracker.as_ref() {
                let (requests, credits, rpm) = tracker.relay_summary(&r.name);
                v.requests = requests;
                v.credits = credits;
                v.rpm = rpm;
            }
            v
        })
        .collect();
    Json(views).into_response()
}

/// POST /api/admin/relays — 新建中转
pub async fn create_relay(
    State(state): State<AdminState>,
    Json(payload): Json<CreateRelayRequest>,
) -> Response {
    let Some(mgr) = state.relay_manager.as_ref() else {
        return no_relay();
    };
    if payload.name.trim().is_empty()
        || payload.base_url.trim().is_empty()
        || payload.api_key.trim().is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!(AdminErrorResponse::invalid_request(
                "name / baseUrl / apiKey 均不能为空"
            ))),
        )
            .into_response();
    }
    let cfg = mgr.create(payload);
    Json(cfg.to_masked()).into_response()
}

/// PUT /api/admin/relays/:id — 更新中转
pub async fn update_relay(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateRelayRequest>,
) -> Response {
    let Some(mgr) = state.relay_manager.as_ref() else {
        return no_relay();
    };
    match mgr.update(id, payload) {
        Ok(cfg) => Json(cfg.to_masked()).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!(AdminErrorResponse::not_found(e.to_string()))),
        )
            .into_response(),
    }
}

/// DELETE /api/admin/relays/:id — 删除中转
pub async fn delete_relay(State(state): State<AdminState>, Path(id): Path<u64>) -> Response {
    let Some(mgr) = state.relay_manager.as_ref() else {
        return no_relay();
    };
    match mgr.delete(id) {
        Ok(_) => Json(SuccessResponse::new(format!("中转 #{} 已删除", id))).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!(AdminErrorResponse::not_found(e.to_string()))),
        )
            .into_response(),
    }
}

/// POST /api/admin/relays/:id/models — 拉取该中转的模型列表并缓存
pub async fn fetch_relay_models(State(state): State<AdminState>, Path(id): Path<u64>) -> Response {
    let Some(mgr) = state.relay_manager.as_ref() else {
        return no_relay();
    };
    let Some(cfg) = mgr.get(id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!(AdminErrorResponse::not_found(format!(
                "中转 #{} 不存在",
                id
            )))),
        )
            .into_response();
    };
    match forward::fetch_models(mgr, &cfg).await {
        Ok(models) => {
            mgr.set_models(id, models.clone());
            Json(RelayModelsResponse { models }).into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!(AdminErrorResponse::api_error(e.to_string()))),
        )
            .into_response(),
    }
}
