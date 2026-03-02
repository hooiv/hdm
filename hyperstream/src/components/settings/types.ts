export interface SettingsData {
  download_dir: string;
  max_concurrent_downloads: number;
  proxy_enabled: boolean;
  proxy_url: string;
  theme: string;
  vpn_mode: boolean;
  chaos_mode: boolean;
  speed_limit_enabled: boolean;
  speed_limit_rate: number;

  // Cloud
  cloud_enabled: boolean;
  cloud_endpoint: string;
  cloud_bucket: string;
  cloud_region: string;
  cloud_access_key: string;
  cloud_secret_key: string;

  // Advanced / Chaos
  chaos_latency_ms: number;
  chaos_error_rate: number;

  // Privacy
  use_tor: boolean;

  // Team Sync
  last_sync_host?: string;

  // Archive Extraction
  auto_extract_archives?: boolean;
  cleanup_archives_after_extract?: boolean;

  // ChatOps
  telegram_bot_token?: string;
  telegram_chat_id?: string;
  chatops_enabled?: boolean;

  // VPN
  vpn_auto_connect: boolean;
  vpn_connection_name: string;

  // MQTT
  mqtt_enabled: boolean;
  mqtt_broker_url: string;
  mqtt_topic: string;

  // Smart Sleep
  prevent_sleep_during_download: boolean;
  pause_on_low_battery: boolean;
}

export interface WebhookConfig {
  id: string;
  name: string;
  url: string;
  events: string[];
  template: string;
  enabled: boolean;
  max_retries: number;
}
