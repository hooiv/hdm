use reqwest::header::{HeaderMap, HeaderValue};

/// Helper struct to construct headers in a specific order
/// Note: reqwest's HeaderMap does preserve insertion order for iteration, 
/// but standard HashMap does not. We need to be careful.
/// 
/// This struct essentially builds a HeaderMap but ensures we insert key headers first.
pub struct ChromeHeaders;

impl ChromeHeaders {
    pub fn build() -> HeaderMap {
        let mut headers = HeaderMap::with_capacity(15);
        
        // Host is usually added by reqwest automatically, but we can try to influence order
        // connection is next
        
        // 1. Connection (often 'keep-alive') - reqwest adds this
        
        // 2. Sec-CH-UA
        headers.insert(
            "Sec-Ch-Ua",
            HeaderValue::from_static("\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
        );
        
        // 3. Sec-CH-UA-Mobile
        headers.insert(
            "Sec-Ch-Ua-Mobile", 
            HeaderValue::from_static("?0")
        );
        
        // 4. Sec-CH-UA-Platform
        headers.insert(
            "Sec-Ch-Ua-Platform", 
            HeaderValue::from_static("\"Windows\"")
        );
        
        // 5. Upgrade-Insecure-Requests
        headers.insert(
            "Upgrade-Insecure-Requests", 
            HeaderValue::from_static("1")
        );
        
        // 6. User-Agent
        headers.insert(
            reqwest::header::USER_AGENT, 
            HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        );
        
        // 7. Accept
        headers.insert(
            reqwest::header::ACCEPT, 
            HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7")
        );
        
        // 8. Sec-Fetch-Site
        headers.insert(
            "Sec-Fetch-Site", 
            HeaderValue::from_static("none")
        );
        
        // 9. Sec-Fetch-Mode
        headers.insert(
            "Sec-Fetch-Mode", 
            HeaderValue::from_static("navigate")
        );
        
        // 10. Sec-Fetch-User
        headers.insert(
            "Sec-Fetch-User", 
            HeaderValue::from_static("?1")
        );
        
        // 11. Sec-Fetch-Dest
        headers.insert(
            "Sec-Fetch-Dest", 
            HeaderValue::from_static("document")
        );
        
        // 12. Accept-Encoding
        headers.insert(
            reqwest::header::ACCEPT_ENCODING, 
            HeaderValue::from_static("gzip, deflate, br")
        );

        // 13. Accept-Language
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE, 
            HeaderValue::from_static("en-US,en;q=0.9")
        );

        headers
    }
}
