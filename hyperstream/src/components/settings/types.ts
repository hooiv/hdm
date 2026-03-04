import type { AppSettings } from '../../types';

/**
 * SettingsData extends the canonical AppSettings interface
 * with an index signature to tolerate extra/unknown fields
 * from the Rust backend (forward-compatible deserialization).
 */
export interface SettingsData extends AppSettings {
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
