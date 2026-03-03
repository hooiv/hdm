export interface SettingsData {
  download_dir: string;
  segments: number;
  proxy_enabled: boolean;
  proxy_type: string;
  proxy_host: string;
  proxy_port: number;
  speed_limit_kbps: number;

  // Cloud
  cloud_enabled: boolean;
  cloud_endpoint: string;
  cloud_bucket: string;
  cloud_region: string;
  cloud_access_key: string;
  cloud_secret_key: string;

  // Privacy
  use_tor: boolean;
  dpi_evasion: boolean;
  ja3_enabled: boolean;

  // Threads
  min_threads: number;
  max_threads: number;

  // Clipboard
  clipboard_monitor: boolean;
  auto_start_extension: boolean;

  // Categories
  use_category_folders: boolean;

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
  vpn_connection_name?: string;

  // MQTT
  mqtt_enabled: boolean;
  mqtt_broker_url: string;
  mqtt_topic: string;

  // Smart Sleep
  prevent_sleep_during_download: boolean;
  pause_on_low_battery: boolean;

  // P2P
  p2p_enabled: boolean;
  p2p_upload_limit_kbps?: number;

  // Metadata
  auto_scrub_metadata: boolean;

  // Allow extra fields from Rust that aren't explicitly listed
  [key: string]: unknown;
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
