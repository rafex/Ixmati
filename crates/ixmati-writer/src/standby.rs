use std::sync::Arc;
use std::time::Duration;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::Serialize;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct Heartbeat {
    pub writer_id: String,
    pub store: String,
    pub seq: u64,
    pub role: String,
}

pub struct StandbyConfig {
    pub heartbeat_interval: Duration,
    pub standby_timeout: Duration,
}

impl Default for StandbyConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(1000),
            standby_timeout: Duration::from_millis(5000),
        }
    }
}

pub enum WriterRole {
    Active,
    Standby,
}

pub struct StandbyController {
    store: String,
    writer_id: String,
    config: StandbyConfig,
    role: Arc<Mutex<WriterRole>>,
}

impl StandbyController {
    pub fn new(store: &str, writer_id: &str, config: StandbyConfig) -> Self {
        Self {
            store: store.to_string(),
            writer_id: writer_id.to_string(),
            config,
            role: Arc::new(Mutex::new(WriterRole::Standby)),
        }
    }

    pub fn role(&self) -> Arc<Mutex<WriterRole>> {
        Arc::clone(&self.role)
    }

    pub async fn run(&self) -> ! {
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(
            &std::env::var("MQTT_BROKER").unwrap_or_else(|_| "tcp://localhost:1883".into()),
        );

        let mut mqtt_opts = MqttOptions::new(format!("standby-{}", self.writer_id), &host, port);
        mqtt_opts.set_keep_alive(Duration::from_secs(5));

        let topic = format!("ixmati/hb/{}", self.store);

        let (client, mut eventloop) = AsyncClient::new(mqtt_opts, 100);

        client
            .subscribe(&topic, QoS::AtLeastOnce)
            .await
            .expect("standby: failed to subscribe to heartbeat topic");

        let client = Arc::new(client);
        let role = Arc::clone(&self.role);
        let store = self.store.clone();
        let writer_id = self.writer_id.clone();
        let interval = self.config.heartbeat_interval;
        let timeout = self.config.standby_timeout;

        let publish_client = Arc::clone(&client);

        tokio::spawn(async move {
            let mut seq = 0u64;

            loop {
                tokio::time::sleep(interval).await;

                let is_active = matches!(*role.lock().await, WriterRole::Active);

                let hb = Heartbeat {
                    writer_id: writer_id.clone(),
                    store: store.clone(),
                    seq,
                    role: if is_active {
                        "active".into()
                    } else {
                        "standby".into()
                    },
                };

                let payload = serde_json::to_vec(&hb).expect("heartbeat serialization");
                seq += 1;

                if let Err(e) = publish_client
                    .publish(&topic, QoS::AtLeastOnce, false, payload)
                    .await
                {
                    tracing::warn!(error = %e, "standby: heartbeat publish failed");
                }
            }
        });

        let mut last_heartbeat = tokio::time::Instant::now();

        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::Publish(_publish))) => {
                    last_heartbeat = tokio::time::Instant::now();
                    tracing::debug!(store = %self.store, "standby: received heartbeat");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "standby: eventloop error");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }

            let elapsed = last_heartbeat.elapsed();
            let mut role_guard = self.role.lock().await;

            match &*role_guard {
                WriterRole::Standby => {
                    if elapsed < timeout {
                        tracing::debug!(
                            store = %self.store,
                            elapsed_ms = elapsed.as_millis(),
                            "standby: waiting"
                        );
                    } else {
                        *role_guard = WriterRole::Active;
                        tracing::info!(
                            store = %self.store,
                            timeout_ms = timeout.as_millis(),
                            "standby: promoted to ACTIVE"
                        );
                    }
                }
                WriterRole::Active => {
                    if elapsed < timeout {
                        // Another writer is alive — demote if we were active
                        // This handles split-brain: if a primary comes back, we yield
                        if elapsed < self.config.heartbeat_interval * 2 {
                            *role_guard = WriterRole::Standby;
                            tracing::warn!(
                                store = %self.store,
                                "standby: primary detected, demoting to STANDBY"
                            );
                        }
                    }
                }
            }
        }
    }
}
