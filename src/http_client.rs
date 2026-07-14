// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! HTTP Client 构建模块
//!
//! 提供统一的 HTTP Client 构建功能，支持代理配置

use reqwest::{Client, Proxy};
use std::time::Duration;

use crate::model::config::TlsBackend;

/// 代理配置
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ProxyConfig {
    /// 代理地址，支持 http/https/socks5
    pub url: String,
    /// 代理认证用户名
    pub username: Option<String>,
    /// 代理认证密码
    pub password: Option<String>,
}

impl ProxyConfig {
    /// 从 url 创建代理配置
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            username: None,
            password: None,
        }
    }

    /// 设置认证信息
    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }
}

/// 构建 HTTP Client
///
/// # Arguments
/// * `proxy` - 可选的代理配置
/// * `timeout_secs` - 读空闲超时（秒）：相邻两次读取之间的最大间隔，用于流式响应。
///   建连超时取 `min(timeout_secs, 30)`。不设整体请求超时，避免长流被误杀。
///
/// # Returns
/// 配置好的 reqwest::Client
pub fn build_client(
    proxy: Option<&ProxyConfig>,
    timeout_secs: u64,
    tls_backend: TlsBackend,
) -> anyhow::Result<Client> {
    // 不再使用 reqwest 的整体 `.timeout()`。
    //
    // `.timeout()` 覆盖「从建立连接直到把响应体完整读完」的总时长——对流式 SSE 响应，
    // 这意味着整条流必须在 timeout_secs 内读完，否则被主动掐断。长思考 + 长输出的请求
    // 很容易超过该上限，表现为上游流在中途断开（下游插件侧观测到 aborted / read ECONNRESET），
    // 甚至「思考完还没吐正文就被砍」（reqwest 在 reasoning 之后、正文之前到点 abort）。
    //
    // 改用两段更贴合流式语义的超时，总时长不再设硬上限：
    //   - connect_timeout：建立连接的上限（快速失败，不受慢/长响应影响）
    //   - read_timeout：相邻两次读取之间的最大空闲间隔（仅当上游真正卡住无数据时才触发）
    // 只要上游持续产出数据，长响应就不会被误杀；短请求（如 token 刷新）行为与原整体超时等价。
    let connect_timeout_secs = timeout_secs.min(30);
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .read_timeout(Duration::from_secs(timeout_secs))
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30));

    if tls_backend == TlsBackend::Rustls {
        builder = builder.use_rustls_tls();
    }

    if let Some(proxy_config) = proxy {
        let mut proxy = Proxy::all(&proxy_config.url)?;

        // 设置代理认证
        if let (Some(username), Some(password)) = (&proxy_config.username, &proxy_config.password) {
            proxy = proxy.basic_auth(username, password);
        }

        builder = builder.proxy(proxy);
        tracing::debug!("HTTP Client 使用代理: {}", proxy_config.url);
    }

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_new() {
        let config = ProxyConfig::new("http://127.0.0.1:7890");
        assert_eq!(config.url, "http://127.0.0.1:7890");
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_proxy_config_with_auth() {
        let config = ProxyConfig::new("socks5://127.0.0.1:1080").with_auth("user", "pass");
        assert_eq!(config.url, "socks5://127.0.0.1:1080");
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[test]
    fn test_build_client_without_proxy() {
        let client = build_client(None, 30, TlsBackend::Rustls);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_with_proxy() {
        let config = ProxyConfig::new("http://127.0.0.1:7890");
        let client = build_client(Some(&config), 30, TlsBackend::Rustls);
        assert!(client.is_ok());
    }
}
