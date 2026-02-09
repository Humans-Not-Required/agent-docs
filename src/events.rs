use rocket::serde::json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Broadcast channel for SSE events within a workspace.
/// Global bus — clients filter by workspace_id on the stream.
#[derive(Clone)]
pub struct EventBus {
    sender: Arc<broadcast::Sender<SseEvent>>,
}

#[derive(Clone, Debug)]
pub struct SseEvent {
    pub workspace_id: String,
    pub event_type: String,
    pub data: Value,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        EventBus {
            sender: Arc::new(sender),
        }
    }

    pub fn emit(&self, workspace_id: &str, event_type: &str, data: Value) {
        let _ = self.sender.send(SseEvent {
            workspace_id: workspace_id.to_string(),
            event_type: event_type.to_string(),
            data,
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SseEvent> {
        self.sender.subscribe()
    }
}
