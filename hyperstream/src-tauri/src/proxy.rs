use reqwest::{Client, Proxy as ReqwestProxy};
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
            .danger_accept_invalid_certs(true); // TODO: Make this configurable

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
