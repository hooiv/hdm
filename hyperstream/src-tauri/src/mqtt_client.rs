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
        // Parse the broker URL (format: mqtt://host:port)
        // A simple URL parser or string split since url crate might not be in scope for this specifically
        let mut host = "localhost".to_string();
        let mut port = 1883;

        let url_parts: Vec<&str> = broker_url.split("://").collect();
        if url_parts.len() == 2 {
            let host_port: Vec<&str> = url_parts[1].split(':').collect();
            if !host_port.is_empty() {
                host = host_port[0].to_string();
                if host_port.len() > 1 {
                    if let Ok(p) = host_port[1].parse::<u16>() {
                        port = p;
                    }
                }
            }
        }

        let mut mqttoptions = MqttOptions::new("hyperstream_client", host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(5));

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
                    println!("DEBUG: MQTT event published successfully");
                }
                Err(e) => {
                    eprintln!("WARNING: Failed to publish MQTT event: {}", e);
                }
            }
            
            // Allow a short time for the message to be sent before the thread exits
            // rumqttc requires the connection to be iterated to make progress
            let _ = connection.iter().next();
        }
    });
}
