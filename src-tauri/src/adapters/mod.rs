//! Adapter interface (card §4). Per-app on/off maps to `id()`.
pub mod cursor;

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolState { Idle, Working, NeedsYou, Done }

pub trait Adapter: Send {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn is_target_running(&self) -> bool;
    fn poll(&mut self) -> Option<ToolState>;
    fn last_signal_at(&self) -> Instant;
}
