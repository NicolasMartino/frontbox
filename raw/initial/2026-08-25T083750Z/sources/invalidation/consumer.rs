//! Kafka consumer for invalidation fan-out topic.
//!
//! Every fullstack instance uses a unique consumer group so each instance
//! receives every invalidation message and can notify its local SSE clients.

use std::sync::Arc;

use tokio::sync::broadcast;
use tokio::sync::broadcast::error::TryRecvError;
use tracing::{debug, error, info, instrument};

use messaging::{
    create_dlq_publisher, ConsumerConfig, HandleResult, KafkaConsumer, KafkaMessage,
    MessageHandler, MessagingError, Topics,
};
use shared::frontend::sse::InvalidationEvent;

const INVALIDATION_CONSUMER_RETRY_SECS: u64 = 5;

/// Consumer that fans invalidation events into a process-local broadcast channel.
pub struct InvalidationConsumer {
    invalidation_tx: broadcast::Sender<InvalidationEvent>,
    shutdown_tx: broadcast::Sender<()>,
}

impl InvalidationConsumer {
    /// Create a new invalidation consumer and its first receiver.
    pub fn new() -> (Self, broadcast::Receiver<InvalidationEvent>) {
        let (invalidation_tx, invalidation_rx) = broadcast::channel(1000);
        let (shutdown_tx, _) = broadcast::channel(1);

        (
            Self {
                invalidation_tx,
                shutdown_tx,
            },
            invalidation_rx,
        )
    }

    /// Get the sender for sharing with `ServerState`.
    pub fn sender(&self) -> broadcast::Sender<InvalidationEvent> {
        self.invalidation_tx.clone()
    }

    /// Start consuming invalidation events in the background with automatic restart.
    #[instrument(skip(self))]
    pub fn start(&self, bootstrap_servers: String, group_id: String) {
        let invalidation_tx = self.invalidation_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let log_group_id = group_id.clone();

        tokio::spawn(async move {
            loop {
                if shutdown_requested(&mut shutdown_rx) {
                    info!(group_id = %group_id, "Invalidation consumer supervisor stopped");
                    return;
                }

                match run_consumer(
                    &bootstrap_servers,
                    &group_id,
                    invalidation_tx.clone(),
                    shutdown_rx.resubscribe(),
                )
                .await
                {
                    Ok(()) => {
                        if shutdown_requested(&mut shutdown_rx) {
                            info!(group_id = %group_id, "Invalidation consumer stopped");
                            return;
                        }

                        error!(
                            group_id = %group_id,
                            retry_secs = INVALIDATION_CONSUMER_RETRY_SECS,
                            "Invalidation consumer exited unexpectedly; retrying"
                        );
                    }
                    Err(error) => {
                        error!(
                            group_id = %group_id,
                            error = %error,
                            retry_secs = INVALIDATION_CONSUMER_RETRY_SECS,
                            "Invalidation consumer unavailable; retrying"
                        );
                    }
                }

                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(INVALIDATION_CONSUMER_RETRY_SECS)) => {}
                    result = shutdown_rx.recv() => {
                        match result {
                            Ok(()) | Err(broadcast::error::RecvError::Closed) => {
                                info!(group_id = %group_id, "Invalidation consumer supervisor shutting down");
                                return;
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                info!(group_id = %group_id, "Invalidation consumer supervisor shutting down after lagged shutdown signal");
                                return;
                            }
                        }
                    }
                }
            }
        });

        info!(group_id = %log_group_id, "Invalidation consumer supervisor started");
    }
}

async fn run_consumer(
    bootstrap_servers: &str,
    group_id: &str,
    invalidation_tx: broadcast::Sender<InvalidationEvent>,
    shutdown_rx: broadcast::Receiver<()>,
) -> Result<(), MessagingError> {
    let config = ConsumerConfig::new(
        bootstrap_servers,
        group_id,
        vec![Topics::INVALIDATIONS_V1.to_string()],
    );

    let dlq_publisher = match create_dlq_publisher(bootstrap_servers).await {
        Ok(publisher) => Some(Arc::new(publisher)),
        Err(error) => {
            error!(
                error = %error,
                "Failed to create DLQ publisher for invalidation consumer"
            );
            None
        }
    };

    let consumer = KafkaConsumer::new(config).await?;
    let handler = InvalidationHandler { invalidation_tx };

    info!(group_id = %group_id, "Invalidation consumer started");
    consumer
        .run(Arc::new(handler), dlq_publisher, shutdown_rx)
        .await
}

fn shutdown_requested(shutdown_rx: &mut broadcast::Receiver<()>) -> bool {
    match shutdown_rx.try_recv() {
        Ok(()) | Err(TryRecvError::Closed) | Err(TryRecvError::Lagged(_)) => true,
        Err(TryRecvError::Empty) => false,
    }
}

/// Derive a stable-ish invalidation consumer group ID for this instance.
pub fn consumer_group_for_instance(instance_id: &str) -> String {
    let sanitized: String = instance_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let suffix = if sanitized.trim_matches('-').trim().is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    };

    format!("app-invalidations-consumer-{suffix}")
}

struct InvalidationHandler {
    invalidation_tx: broadcast::Sender<InvalidationEvent>,
}

#[async_trait::async_trait]
impl MessageHandler for InvalidationHandler {
    async fn handle(&self, message: &KafkaMessage) -> Result<HandleResult, MessagingError> {
        let event: InvalidationEvent = match message.value_json() {
            Ok(v) => v,
            Err(e) => {
                return Ok(HandleResult::fatal(format!(
                    "Failed to parse invalidation message: {}",
                    e
                )));
            }
        };

        debug!(
            user_id = %event.user_id,
            entity = %event.entity,
            version = event.version,
            "Received invalidation event"
        );

        // Ignore send errors (no receivers is fine).
        let _ = self.invalidation_tx.send(event);
        Ok(HandleResult::ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};
    use uuid::Uuid;

    #[test]
    fn consumer_group_sanitizes_instance_id() {
        let group = consumer_group_for_instance("fullstack/a#1");
        assert_eq!(group, "app-invalidations-consumer-fullstack-a-1");
    }

    #[test]
    fn consumer_group_uses_unknown_suffix_when_blank() {
        assert_eq!(
            consumer_group_for_instance(" \n\t"),
            "app-invalidations-consumer-unknown"
        );
    }

    #[tokio::test]
    async fn handler_broadcasts_valid_invalidation_event() {
        let (consumer, mut rx) = InvalidationConsumer::new();
        let handler = InvalidationHandler {
            invalidation_tx: consumer.sender(),
        };
        let event = InvalidationEvent {
            entity: "exercises".to_string(),
            version: 7,
            user_id: Uuid::new_v4(),
        };
        let message = KafkaMessage {
            topic: Topics::INVALIDATIONS_V1.to_string(),
            partition: 0,
            offset: 12,
            key: Some(event.user_id.to_string().into_bytes()),
            value: Some(serde_json::to_vec(&event).expect("serialize event")),
            headers: vec![],
            timestamp: chrono::Utc::now(),
        };

        let result = handler.handle(&message).await.expect("handle invalidation");
        let received = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("receive invalidation")
            .expect("broadcast payload");

        assert!(matches!(result, HandleResult::Ok));
        assert_eq!(received, event);
    }

    #[tokio::test]
    async fn handler_marks_invalid_json_as_fatal() {
        let (consumer, _) = InvalidationConsumer::new();
        let handler = InvalidationHandler {
            invalidation_tx: consumer.sender(),
        };
        let message = KafkaMessage {
            topic: Topics::INVALIDATIONS_V1.to_string(),
            partition: 0,
            offset: 1,
            key: None,
            value: Some(b"{not-json}".to_vec()),
            headers: vec![],
            timestamp: chrono::Utc::now(),
        };

        let result = handler
            .handle(&message)
            .await
            .expect("handle invalid payload");

        assert!(
            matches!(result, HandleResult::FatalError(message) if message.contains("Failed to parse invalidation message"))
        );
    }

    #[tokio::test]
    async fn sender_broadcasts_to_initial_receiver() {
        let (consumer, mut rx) = InvalidationConsumer::new();
        let event = InvalidationEvent {
            entity: "sessions".to_string(),
            version: 3,
            user_id: Uuid::new_v4(),
        };

        consumer
            .sender()
            .send(event.clone())
            .expect("broadcast invalidation event");

        let received = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("wait for invalidation broadcast")
            .expect("broadcast event");

        assert_eq!(received, event);
    }

    #[test]
    fn shutdown_requested_is_false_without_signal() {
        let (tx, mut rx) = broadcast::channel(1);
        let _keep_sender_alive = tx;

        assert!(!shutdown_requested(&mut rx));
    }

    #[test]
    fn shutdown_requested_is_true_after_signal() {
        let (tx, mut rx) = broadcast::channel(1);
        tx.send(()).expect("send shutdown signal");

        assert!(shutdown_requested(&mut rx));
    }
}
