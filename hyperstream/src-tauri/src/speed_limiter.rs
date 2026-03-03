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
    /// Timestamp of when the next refill is allowed (microseconds from app start)
    next_refill_micros: AtomicU64,
}

lazy_static::lazy_static! {
    static ref LIMITER_START: std::time::Instant = std::time::Instant::now();
}

impl SpeedLimiter {
    pub fn new() -> Self {
        // Ensure start time is initialized
        let _ = *LIMITER_START;
        
        Self {
            limit_bytes_per_sec: AtomicU64::new(0),
            tokens: AtomicU64::new(0),
            last_refill: std::sync::Mutex::new(Instant::now()),
            enabled: AtomicBool::new(false),
            next_refill_micros: AtomicU64::new(0),
        }
    }

    fn now_micros() -> u64 {
        std::time::Instant::now().duration_since(*LIMITER_START).as_micros() as u64
    }

    /// Set speed limit in bytes per second (0 = unlimited)
    pub fn set_limit(&self, bytes_per_sec: u64) {
        self.limit_bytes_per_sec.store(bytes_per_sec, Ordering::SeqCst);
        self.enabled.store(bytes_per_sec > 0, Ordering::SeqCst);
        // Give initial tokens
        self.tokens.store(bytes_per_sec, Ordering::SeqCst);
        self.next_refill_micros.store(Self::now_micros(), Ordering::SeqCst);
    }

    /// Get current limit
    pub fn get_limit(&self) -> u64 {
        self.limit_bytes_per_sec.load(Ordering::SeqCst)
    }

    /// Request to consume bytes. Returns how many bytes are allowed.
    /// Blocks if necessary to respect the rate limit.
    pub async fn acquire(&self, requested_bytes: u64) -> u64 {
        if !self.enabled.load(Ordering::Relaxed) {
             return requested_bytes;
        }

        let limit = self.limit_bytes_per_sec.load(Ordering::Relaxed);
        if limit == 0 {
            return requested_bytes;
        }
        
        // Wait until we can acquire some tokens
        loop {
             let now = Self::now_micros();
             let next_refill = self.next_refill_micros.load(Ordering::Relaxed);

             // 1. Try to refill ONLY if it's time (Lock-Free Check)
             if now >= next_refill {
                 // Double-check optimization: minimal lock duration
                 if let Ok(mut last) = self.last_refill.try_lock() {
                     let now_instant = Instant::now();
                     let elapsed = now_instant.duration_since(*last);
                     
                     if elapsed >= Duration::from_millis(50) {
                         let refill_amount = (limit as f64 * elapsed.as_secs_f64()) as u64;
                         if refill_amount > 0 {
                             let current = self.tokens.load(Ordering::Acquire);
                             // Cap at 1.0 seconds worth to prevent huge bursts after idle
                             let new_tokens = (current + refill_amount).min(limit); 
                             self.tokens.store(new_tokens, Ordering::Release);
                             *last = now_instant;
                             
                             // Update next hint (current time + 50ms)
                             self.next_refill_micros.store(Self::now_micros() + 50_000, Ordering::Release);
                         }
                     }
                 }
             }

             // 2. Try to consume
             let current = self.tokens.load(Ordering::Acquire);
             if current > 0 {
                 let to_consume = requested_bytes.min(current);
                 // CAS loop to safely subtract
                 match self.tokens.compare_exchange(
                     current, 
                     current - to_consume, 
                     Ordering::SeqCst, 
                     Ordering::Relaxed
                 ) {
                     Ok(_) => return to_consume,
                     Err(_new_current) => {
                         // Race condition lost, try again immediately with new value
                         continue;
                     }
                 }
             }

             // 3. Not enough tokens, wait (20ms)
             tokio::time::sleep(Duration::from_millis(20)).await;
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
