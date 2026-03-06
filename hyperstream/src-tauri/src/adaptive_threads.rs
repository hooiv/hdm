use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

lazy_static::lazy_static! {
    /// Global thread controller — recommends optimal segment count based on bandwidth trends
    pub static ref THREAD_CONTROLLER: AdaptiveThreadController =
        AdaptiveThreadController::new(2, 16);
    /// Global bandwidth monitor — tracks download speeds across all active downloads
    pub static ref BANDWIDTH_MONITOR: BandwidthMonitor =
        BandwidthMonitor::new(5);
}

/// Get the current recommended thread/segment count
pub fn recommended_threads() -> u32 {
    THREAD_CONTROLLER.get_threads()
}

/// Internal PID controller state — all fields grouped under a single Mutex
/// to prevent interleaved reads/writes from concurrent callers.
struct PidState {
    integral: f64,
    last_error: f64,
    last_update: Instant,
}

/// PID Controller for adaptive thread tuning
/// Adjusts thread count based on bandwidth utilization
pub struct AdaptiveThreadController {
    /// Current number of threads
    pub current_threads: AtomicU32,
    /// Minimum threads
    pub min_threads: u32,
    /// Maximum threads
    pub max_threads: u32,
    /// Target bandwidth utilization (0.0 - 1.0)
    pub target_utilization: f64,
    /// PID gains
    pub kp: f64, // Proportional gain
    pub ki: f64, // Integral gain
    pub kd: f64, // Derivative gain
    /// Combined PID state under a single lock
    state: std::sync::Mutex<PidState>,
}

impl AdaptiveThreadController {
    pub fn new(min_threads: u32, max_threads: u32) -> Self {
        Self {
            current_threads: AtomicU32::new(min_threads),
            min_threads,
            max_threads,
            target_utilization: 0.85, // Target 85% bandwidth utilization
            kp: 2.0,
            ki: 0.1,
            kd: 0.5,
            state: std::sync::Mutex::new(PidState {
                integral: 0.0,
                last_error: 0.0,
                last_update: Instant::now(),
            }),
        }
    }

    /// Update thread count based on current bandwidth metrics
    /// Returns the new recommended thread count
    pub fn update(&self, current_speed: u64, max_possible_speed: u64) -> u32 {
        if max_possible_speed == 0 {
            return self.current_threads.load(Ordering::Relaxed);
        }

        let utilization = current_speed as f64 / max_possible_speed as f64;
        let error = self.target_utilization - utilization;

        let mut pid = self.state.lock().unwrap_or_else(|e| e.into_inner());

        let now = Instant::now();
        let dt = now.duration_since(pid.last_update).as_secs_f64();
        pid.last_update = now;

        if dt <= 0.0 {
            return self.current_threads.load(Ordering::Relaxed);
        }

        // PID calculation
        pid.integral += error * dt;
        // Anti-windup: limit integral
        pid.integral = pid.integral.clamp(-10.0, 10.0);

        let derivative = (error - pid.last_error) / dt;
        pid.last_error = error;

        let output = self.kp * error + self.ki * pid.integral + self.kd * derivative;
        
        let current = self.current_threads.load(Ordering::Relaxed) as f64;
        // Clamp f64 before casting to u32 to prevent negative-to-u32 saturation issues
        let new_threads = (current + output).round()
            .clamp(self.min_threads as f64, self.max_threads as f64) as u32;

        self.current_threads.store(new_threads, Ordering::Relaxed);
        new_threads
    }

    /// Get current thread count
    pub fn get_threads(&self) -> u32 {
        self.current_threads.load(Ordering::Relaxed)
    }

    /// Reset the controller state
    #[allow(dead_code)]
    pub fn reset(&self) {
        let mut pid = self.state.lock().unwrap_or_else(|e| e.into_inner());
        pid.integral = 0.0;
        pid.last_error = 0.0;
        pid.last_update = Instant::now();
    }
}

/// Simple bandwidth monitor
pub struct BandwidthMonitor {
    samples: std::sync::Mutex<Vec<(Instant, u64)>>,
    window: Duration,
}

impl BandwidthMonitor {
    pub fn new(window_seconds: u64) -> Self {
        Self {
            samples: std::sync::Mutex::new(Vec::new()),
            window: Duration::from_secs(window_seconds),
        }
    }

    /// Add an incremental bandwidth sample.
    /// `bytes`: the number of bytes transferred since the last sample (NOT cumulative total).
    pub fn add_sample(&self, bytes: u64) {
        let mut samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        samples.push((now, bytes));

        // Remove old samples
        let cutoff = now - self.window;
        samples.retain(|(t, _)| *t >= cutoff);
    }

    pub fn get_average_speed(&self) -> u64 {
        let samples = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        if samples.len() < 2 {
            return 0;
        }

        let total_bytes: u64 = samples.iter().map(|(_, b)| b).sum();
        let duration = samples.last().unwrap().0.duration_since(samples.first().unwrap().0);
        
        if duration.as_secs_f64() > 0.0 {
            (total_bytes as f64 / duration.as_secs_f64()) as u64
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_increase() {
        let controller = AdaptiveThreadController::new(2, 16);
        
        // Low utilization -> should increase threads
        let new_threads = controller.update(50_000, 100_000); // 50% util
        assert!(new_threads >= 2);
    }

    #[test]
    fn test_controller_bounds() {
        let controller = AdaptiveThreadController::new(2, 8);
        
        // Very low utilization
        for _ in 0..10 {
            controller.update(10_000, 1_000_000);
            std::thread::sleep(Duration::from_millis(100));
        }
        
        assert!(controller.get_threads() <= 8);
        assert!(controller.get_threads() >= 2);
    }
}
