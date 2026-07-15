// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 中转对接（备用路由）模块
//!
//! 管理任意 Anthropic 兼容中转（如 sub2api），支持：
//! - direct：某些模型直接走中转（跳过 Kiro）
//! - fallback：仅当 Kiro 账号池整体失败时兜底走中转
//!
//! 计费记真实中转模型名 + 按 GPT 官方定价结构 × 自标定 credits/USD × 可配倍率。

pub mod forward;
pub mod manager;
pub mod types;

pub use manager::RelayManager;
