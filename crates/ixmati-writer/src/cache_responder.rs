use ixmati_cache::CacheBackend;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct CacheQuery {
    correlation_id: String,
    store: String,
    entity: String,
    key: String,
}

#[derive(Debug, Serialize)]
struct CacheResponse {
    correlation_id: String,
    found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Vec<u8>>,
}

pub struct CacheResponder {
    #[allow(dead_code)]
    cache: Arc<dyn CacheBackend>,
    #[allow(dead_code)]
    client: AsyncClient,
}

impl CacheResponder {
    pub fn new(cache: Arc<dyn CacheBackend>, broker: &str) -> Self {
        let client_id = format!("writer-cache-resp-{}", Uuid::new_v4());
        let (host, port) = ixmati_core::mqtt::parse_mqtt_broker(broker);
        let mut opts = MqttOptions::new(client_id, &host, port);
        opts.set_keep_alive(Duration::from_secs(5));

        let (client, mut eventloop) = AsyncClient::new(opts, 100);
        let cache_clone = Arc::clone(&cache);
        let sub_client = client.clone();

        tokio::spawn(async move {
            sub_client
                .subscribe("ixmati/qry/#", QoS::AtMostOnce)
                .await
                .ok();
            tracing::info!("CacheResponder subscribed to ixmati/qry/#");

            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        let query: CacheQuery =
                            match serde_json::from_slice(&publish.payload) {
                                Ok(q) => q,
                                Err(e) => {
                                    tracing::warn!(error=%e, "CacheResponder: bad payload");
                                    continue;
                                }
                            };

                        tracing::info!(
                            correlation_id=%query.correlation_id,
                            store=%query.store,
                            "CacheResponder: received query"
                        );

                        let cache_key = format!(
                            "c:{}:{}:{}",
                            query.store, query.entity, query.key
                        );
                        let found = cache_clone.get("cache", &cache_key, "");

                        let resp = CacheResponse {
                            correlation_id: query.correlation_id.clone(),
                            found: found.is_some(),
                            payload: found,
                        };
                        let resp_topic = format!("ixmati/qry-resp/{}", query.correlation_id);
                        let resp_payload = serde_json::to_vec(&resp).unwrap_or_default();

                        if let Err(e) = sub_client
                            .publish(resp_topic, QoS::AtMostOnce, false, resp_payload)
                            .await
                        {
                            tracing::error!(error=%e, "CacheResponder: publish response failed");
                        }
                    }
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        sub_client
                            .subscribe("ixmati/qry/#", QoS::AtMostOnce)
                            .await
                            .ok();
                        tracing::info!("CacheResponder: reconnected");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error=%e, "CacheResponder eventloop error");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Self { cache, client }
    }
}
