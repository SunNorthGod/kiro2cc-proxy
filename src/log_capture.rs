// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 内存日志捕获层 — 将 tracing 事件存入 ring buffer 并通过 broadcast 广播

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

const BROADCAST_CAPACITY: usize = 256;

#[derive(Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

pub struct LogCapture {
    ring_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    sender: broadcast::Sender<LogEntry>,
    capacity: usize,
}

impl LogCapture {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            ring_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            sender,
            capacity,
        }
    }

    pub fn as_layer(&self) -> LogCaptureLayer {
        LogCaptureLayer {
            ring_buffer: self.ring_buffer.clone(),
            sender: self.sender.clone(),
            capacity: self.capacity,
        }
    }

    /// 返回当前 ring buffer 快照（全量复制）
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.ring_buffer.lock().iter().cloned().collect()
    }

    /// 订阅后续新事件（返回 broadcast::Receiver）
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.sender.subscribe()
    }
}

pub struct LogCaptureLayer {
    ring_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    sender: broadcast::Sender<LogEntry>,
    capacity: usize,
}

impl<S: Subscriber> Layer<S> for LogCaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        // 防御自放大循环：日志广播的消费端（admin::log_handler 的 SSE 流）自身产生的日志
        // 不纳入捕获/广播。否则该模块一旦记日志（如广播 Lagged 提示），会被本层再次广播 →
        // 该 SSE 流消费不及再次 Lagged → 又记日志 …… 指数爆炸（曾导致 CPU 打满、日志被刷爆）。
        if metadata.target().contains("admin::log_handler") {
            return;
        }

        let level = match *metadata.level() {
            Level::TRACE => "TRACE",
            Level::DEBUG => "DEBUG",
            Level::INFO => "INFO",
            Level::WARN => "WARN",
            Level::ERROR => "ERROR",
        };

        let mut message = String::new();
        event.record(&mut MessageVisitor(&mut message));

        let entry = LogEntry {
            timestamp: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
            level: level.to_string(),
            target: metadata.target().to_string(),
            message,
        };

        {
            let mut buf = self.ring_buffer.lock();
            if buf.len() >= self.capacity {
                buf.pop_front();
            }
            buf.push_back(entry.clone());
        }

        // 无接收者时 send 返回 Err，属正常情况，忽略
        let _ = self.sender.send(entry);
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for MessageVisitor<'a> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(&format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let s = format!("{:?}", value);
        if field.name() == "message" {
            // 去掉 Debug 输出的外层引号（字符串类型会被加引号）
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                self.0.push_str(&s[1..s.len() - 1]);
            } else {
                self.0.push_str(&s);
            }
        } else {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(&format!("{}={}", field.name(), s));
        }
    }
}
