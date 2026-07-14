// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! User API 路由配置

use axum::{
    Router, middleware,
    routing::{get, post},
};

use super::{
    handlers::{get_recharge_records, get_usage, get_usage_records, login},
    middleware::{UserState, reseller_auth_middleware, user_auth_middleware},
    reseller::{
        create_sub_key, delete_sub_key, overview, sub_key_recharges, topup_sub_key, update_sub_key,
    },
};

/// 创建 User API 路由
pub fn create_user_router(state: UserState) -> Router {
    // login 不需要鉴权
    let public = Router::new()
        .route("/login", post(login))
        .with_state(state.clone());

    // usage 需要鉴权
    let protected = Router::new()
        .route("/usage", get(get_usage))
        .route("/usage/records", get(get_usage_records))
        .route("/recharges", get(get_recharge_records))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            user_auth_middleware,
        ))
        .with_state(state.clone());

    // 分销商管理：需要分销卡密鉴权
    let reseller = Router::new()
        .route("/reseller/overview", get(overview))
        .route("/reseller/sub-keys", post(create_sub_key))
        .route(
            "/reseller/sub-keys/{id}",
            axum::routing::put(update_sub_key).delete(delete_sub_key),
        )
        .route("/reseller/sub-keys/{id}/topup", post(topup_sub_key))
        .route(
            "/reseller/sub-keys/{id}/recharges",
            get(sub_key_recharges),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            reseller_auth_middleware,
        ))
        .with_state(state);

    public.merge(protected).merge(reseller)
}
