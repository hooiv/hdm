use rumqttc::{Client, MqttOptions, QoS};
use serde::Serialize;
use std::time::Duration;
use std::thread;

use crate::settings;

#[derive(Serialize)]
pub struct MqttEventPayload {
    pub event: String,
    pub download_id: String,
    pub filename: String,
    pub status: String,
}

pub fn publish_event(event_type: &str, download_id: &str, filename: &str, status: &str) {
    let settings = settings::load_settings();
    
    if !settings.mqtt_enabled {
        return;
    }

    let broker_url = settings.mqtt_broker_url;
    let topic = settings.mqtt_topic;

    if broker_url.trim().is_empty() || topic.trim().is_empty() {
        return;
    }

    let event_type = event_type.to_string();
    let download_id = download_id.to_string();
    let filename = filename.to_string();
    let status = status.to_string();

    // Spawn a thread to handle the publishing so we don't block the async runtime or caller
    thread::spawn(move || {
        // Parse the broker URL (format: mqtt://host:port or mqtts://host:port)
        let mut host = "localhost".to_string();
        let mut port = 1883;
        let mut use_tls = false;

        let url_parts: Vec<&str> = broker_url.split("://").collect();
        if url_parts.len() == 2 {
            let scheme = url_parts[0].to_lowercase();
            use_tls = scheme == "mqtts" || scheme == "ssl" || scheme == "tls";
            let host_port: Vec<&str> = url_parts[1].split(':').collect();
            if !host_port.is_empty() {
                host = host_port[0].to_string();
                if host_port.len() > 1 {
                    if let Ok(p) = host_port[1].parse::<u16>() {
                        port = p;
                    }
                } else if use_tls {
                    port = 8883; // Default MQTTS port
                }
            }
        }

        // SECURITY: Warn when using plaintext MQTT. Event data (filenames, URLs) is visible on the wire.
        if !use_tls {
            eprintln!("⚠️  MQTT: Connecting without TLS to {}:{}. Event data will be sent in plaintext.", host, port);
        }

        let client_id = format!("hyperstream_{}", std::process::id());
        let mut mqttoptions = MqttOptions::new(client_id, &host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(5));
        // Note: NetworkOptions default conn_timeout is already 5 seconds

        // Enable TLS for secure connections
        // Note: rumqttc TLS requires the "use-native-tls" feature to be enabled.
        // Currently not compiled in — refuse to silently downgrade to plaintext.
        if use_tls {
            eprintln!("ERROR: MQTT TLS requested (mqtts://) but TLS support not compiled in. Refusing to send credentials/data in plaintext.");
            return; // Do NOT fall back to plaintext when user explicitly requested TLS
        }

        let (client, mut connection) = Client::new(mqttoptions, 10);

        let payload = MqttEventPayload {
            event: event_type,
            download_id,
            filename,
            status,
        };

        if let Ok(json_bytes) = serde_json::to_vec(&payload) {
            match client.publish(topic, QoS::AtMostOnce, false, json_bytes) {
                Ok(_) => {
                }
                Err(e) => {
                    eprintln!("WARNING: Failed to publish MQTT event: {}", e);
                }
            }
            
            // Drive the event loop long enough for CONNECT, CONNACK, and PUBLISH
            // to be flushed. rumqttc requires iterating the connection to make progress.
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            for notification in connection.iter() {
                if std::time::Instant::now() >= deadline {
                    break;
                }
                match notification {
                    Ok(_) => {
                        // After a few successful events (CONNACK + PUBACK/send), 
                        // we can safely exit
                    }
                    Err(_) => break,
                }
            }
        }
    });
}
