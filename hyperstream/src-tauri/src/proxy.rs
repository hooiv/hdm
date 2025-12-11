use reqwest::{Client, Proxy as ReqwestProxy};
use rquest::{Proxy as RquestProxy};
use serde::{Serialize, Deserialize};

/// Proxy type configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProxyType {
    None,
    Http,
    Https,
    Socks5,
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::None
    }
}

/// Proxy configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Bypass proxy for these hosts (comma-separated)
    pub bypass_list: String,
}

impl ProxyConfig {
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        // Tor Override
        if s.use_tor {
            // Check if Tor is ready (port assigned)
            if let Some(port) = crate::network::tor::get_socks_port() {
                return Self {
                    enabled: true,
                    proxy_type: ProxyType::Socks5,
                    host: "127.0.0.1".to_string(),
                    port,
                    username: None,
                    password: None,
                    bypass_list: String::new(),
                };
            }
        }

        Self {
            enabled: s.proxy_enabled,
            proxy_type: match s.proxy_type.as_str() {
                "socks5" => ProxyType::Socks5,
                "https" => ProxyType::Https,
                _ => ProxyType::Http,
            },
            host: s.proxy_host.clone(),
            port: s.proxy_port,
            username: s.proxy_username.clone(),
            password: s.proxy_password.clone(),
            bypass_list: String::new(), 
        }
    }
    
    /// Build a reqwest Proxy from this config
    pub fn to_reqwest_proxy(&self) -> Option<ReqwestProxy> {
        if !self.enabled || self.host.is_empty() {
            return None;
        }

        let url = match self.proxy_type {
            ProxyType::None => return None,
            ProxyType::Http => format!("http://{}:{}", self.host, self.port),
            ProxyType::Https => format!("https://{}:{}", self.host, self.port),
            ProxyType::Socks5 => format!("socks5://{}:{}", self.host, self.port),
        };

        let proxy = match ReqwestProxy::all(&url) {
            Ok(mut p) => {
                // Add authentication if provided
                if let (Some(user), Some(pass)) = (&self.username, &self.password) {
                    p = p.basic_auth(user, pass);
                }
                Some(p)
            }
            Err(e) => {
                eprintln!("Failed to create proxy: {}", e);
                None
            }
        };

        proxy
    }

    /// Build a rquest Proxy from this config
    pub fn to_rquest_proxy(&self) -> Option<RquestProxy> {
        if !self.enabled || self.host.is_empty() {
            return None;
        }

        let url = match self.proxy_type {
            ProxyType::None => return None,
            ProxyType::Http => format!("http://{}:{}", self.host, self.port),
            ProxyType::Https => format!("https://{}:{}", self.host, self.port),
            ProxyType::Socks5 => format!("socks5://{}:{}", self.host, self.port),
        };

        let proxy = match RquestProxy::all(&url) {
            Ok(mut p) => {
                // Add authentication if provided
                if let (Some(user), Some(pass)) = (&self.username, &self.password) {
                    p = p.basic_auth(user, pass);
                }
                Some(p)
            }
            Err(e) => {
                eprintln!("Failed to create proxy: {}", e);
                None
            }
        };

        proxy
    }

    /// Check if proxy is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.proxy_type != ProxyType::None && !self.host.is_empty()
    }

    /// Check if a host should bypass the proxy
    pub fn should_bypass(&self, host: &str) -> bool {
        if self.bypass_list.is_empty() {
            return false;
        }

        for bypass in self.bypass_list.split(',') {
            let bypass = bypass.trim();
            if bypass.is_empty() {
                continue;
            }
            
            // Support wildcards like *.example.com
            if bypass.starts_with("*.") {
                let domain = &bypass[2..];
                if host.ends_with(domain) || host == domain {
                    return true;
                }
            } else if host == bypass {
                return true;
            }
        }

        false
    }

    /// Build a reqwest Client with this proxy config
    pub fn build_client(&self) -> Result<Client, String> {
        let mut builder = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .min_tls_version(reqwest::tls::Version::TLS_1_2);

        if let Some(proxy) = self.to_reqwest_proxy() {
            builder = builder.proxy(proxy);
        }

        builder.build().map_err(|e| format!("Failed to build client: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bypass_list() {
        let config = ProxyConfig {
            enabled: true,
            proxy_type: ProxyType::Http,
            host: "proxy.example.com".to_string(),
            port: 8080,
            bypass_list: "localhost, *.google.com, 192.168.1.1".to_string(),
            ..Default::default()
        };

        assert!(config.should_bypass("localhost"));
        assert!(config.should_bypass("www.google.com"));
        assert!(config.should_bypass("mail.google.com"));
        assert!(config.should_bypass("192.168.1.1"));
        assert!(!config.should_bypass("example.com"));
    }

    #[test]
    fn test_proxy_url_generation() {
        let config = ProxyConfig {
            enabled: true,
            proxy_type: ProxyType::Socks5,
            host: "127.0.0.1".to_string(),
            port: 1080,
            ..Default::default()
        };

        assert!(config.to_reqwest_proxy().is_some());
    }
}
