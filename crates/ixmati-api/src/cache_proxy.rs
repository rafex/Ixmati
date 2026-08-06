use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct CacheQuery {
    correlation_id: String,
    store: String,
    entity: String,
    key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheResponse {
    correlation_id: String,
    found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct CacheProxy {
    pub(crate) pending: Arc<Mutex<HashMap<String, oneshot::Sender<CacheResponse>>>>,
}

impl CacheProxy {
    pub fn new(broker: &str) -> (Self, AsyncClient) {
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let mut opts = MqttOptions::new(
            format!("api-cache-proxy-{}", Uuid::new_v4()),
            &host,
            port,
        );
        opts.set_keep_alive(Duration::from_secs(5));

        let (client, mut eventloop) = AsyncClient::new(opts, 100);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<CacheResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = Arc::clone(&pending);
        let sub_client = client.clone();

        tokio::spawn(async move {
            let mut retries = 0;
            loop {
                if sub_client
                    .subscribe("ixmati/qry-resp/#", QoS::AtMostOnce)
                    .await
                    .is_err()
                {
                    retries += 1;
                    if retries <= 5 {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                }
                retries = 0;

                loop {
                    match eventloop.poll().await {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            if let Ok(resp) =
                                serde_json::from_slice::<CacheResponse>(&publish.payload)
                            {
                                let mut map = pending_clone.lock().await;
                                if let Some(tx) = map.remove(&resp.correlation_id) {
                                    let _ = tx.send(resp);
                                }
                            }
                        }
                        Ok(Event::Incoming(Packet::ConnAck(_))) => {
                            sub_client
                                .subscribe("ixmati/qry-resp/#", QoS::AtMostOnce)
                                .await
                                .ok();
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "CacheProxy MQTT eventloop error, reconnecting..."
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        });

        (
            Self { pending },
            client,
        )
    }

    pub async fn query(
        &self,
        client: &AsyncClient,
        store: &str,
        entity: &str,
        key: &str,
    ) -> Option<Vec<u8>> {
        let correlation_id = Uuid::new_v4().to_string();
        tracing::info!(correlation_id=%correlation_id, store=%store, entity=%entity, key=%key, "CacheProxy: query");
        let (tx, rx) = oneshot::channel();

        self.pending
            .lock()
            .await
            .insert(correlation_id.clone(), tx);

        let query = CacheQuery {
            correlation_id: correlation_id.clone(),
            store: store.to_string(),
            entity: entity.to_string(),
            key: key.to_string(),
        };

        let payload = serde_json::to_vec(&query).unwrap_or_default();
        if let Err(e) = client
            .publish(
                format!("ixmati/qry/{}/{}/{}", store, entity, key),
                QoS::AtMostOnce,
                false,
                payload,
            )
            .await
        {
            tracing::warn!(error = %e, store=%store, entity=%entity, key=%key, "CacheProxy publish failed");
            self.pending.lock().await.remove(&correlation_id);
            return None;
        }

        let resp = tokio::time::timeout(Duration::from_millis(100), rx).await;

        self.pending.lock().await.remove(&correlation_id);

        match resp {
            Ok(Ok(r)) if r.found => {
                tracing::info!(correlation_id=%correlation_id, "CacheProxy hit");
                r.payload
            }
            Ok(Ok(r)) => {
                tracing::debug!(correlation_id=%correlation_id, "CacheProxy miss");
                None
            }
            Ok(Err(_)) => {
                tracing::debug!(correlation_id=%correlation_id, "CacheProxy sender dropped");
                None
            }
            Err(_) => {
                tracing::debug!(correlation_id=%correlation_id, "CacheProxy timeout");
                None
            }
        }
    }
}
