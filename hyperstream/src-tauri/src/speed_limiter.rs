use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{Duration, Instant};

/// Global speed limiter using Token Bucket algorithm
pub struct SpeedLimiter {
    /// Bytes allowed per second (0 = unlimited)
    limit_bytes_per_sec: AtomicU64,
    /// Current tokens available
    tokens: AtomicU64,
    /// Last refill time
    last_refill: std::sync::Mutex<Instant>,
    /// Is limiting enabled
    enabled: AtomicBool,
}

impl SpeedLimiter {
    pub fn new() -> Self {
        Self {
            limit_bytes_per_sec: AtomicU64::new(0),
            tokens: AtomicU64::new(0),
            last_refill: std::sync::Mutex::new(Instant::now()),
            enabled: AtomicBool::new(false),
        }
    }

    /// Set speed limit in bytes per second (0 = unlimited)
    pub fn set_limit(&self, bytes_per_sec: u64) {
        self.limit_bytes_per_sec.store(bytes_per_sec, Ordering::SeqCst);
        self.enabled.store(bytes_per_sec > 0, Ordering::SeqCst);
        // Give initial tokens
        self.tokens.store(bytes_per_sec, Ordering::SeqCst);
    }

    /// Get current limit
    #[allow(dead_code)]
    pub fn get_limit(&self) -> u64 {
        self.limit_bytes_per_sec.load(Ordering::SeqCst)
    }

    /// Request to consume bytes. Returns how many bytes are allowed.
    /// Blocks if necessary to respect the rate limit.
    pub async fn acquire(&self, requested_bytes: u64) -> u64 {
        if !self.enabled.load(Ordering::SeqCst) {
            return requested_bytes; // No limit
        }

        let limit = self.limit_bytes_per_sec.load(Ordering::SeqCst);
        if limit == 0 {
            return requested_bytes;
        }

        loop {
            // Refill tokens based on elapsed time
            {
                let mut last = self.last_refill.lock().unwrap();
                let now = Instant::now();
                let elapsed = now.duration_since(*last);
                
                // Refill at 10 times per second for smooth limiting
                if elapsed >= Duration::from_millis(100) {
                    let refill_amount = (limit as f64 * elapsed.as_secs_f64()) as u64;
                    let current = self.tokens.load(Ordering::SeqCst);
                    let new_tokens = (current + refill_amount).min(limit * 2); // Cap at 2 seconds worth
                    self.tokens.store(new_tokens, Ordering::SeqCst);
                    *last = now;
                }
            }

            // Try to consume tokens
            let available = self.tokens.load(Ordering::SeqCst);
            
            if available == 0 {
                // Wait for refill (100ms)
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            let to_consume = requested_bytes.min(available);
            self.tokens.fetch_sub(to_consume, Ordering::SeqCst);
            
            return to_consume;
        }
    }
}

impl Default for SpeedLimiter {
    fn default() -> Self {
        Self::new()
    }
}

lazy_static::lazy_static! {
    pub static ref GLOBAL_LIMITER: Arc<SpeedLimiter> = Arc::new(SpeedLimiter::new());
}
