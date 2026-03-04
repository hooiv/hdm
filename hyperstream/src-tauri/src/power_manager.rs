use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, SetThreadExecutionState, SYSTEM_POWER_STATUS,
    ES_CONTINUOUS, ES_SYSTEM_REQUIRED, EXECUTION_STATE
};

static SLEEP_PREVENTED: AtomicBool = AtomicBool::new(false);
static SLEEP_TX: OnceLock<mpsc::Sender<bool>> = OnceLock::new();

/// Spawn a dedicated thread that owns all SetThreadExecutionState calls.
/// This is necessary because SetThreadExecutionState is thread-local —
/// calling it from random thread-pool threads causes sleep prevention
/// to leak (enable on Thread A, disable on Thread B = Thread A stuck).
fn get_sleep_sender() -> &'static mpsc::Sender<bool> {
    SLEEP_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<bool>();
        std::thread::Builder::new()
            .name("sleep-guard".into())
            .spawn(move || {
                while let Ok(prevent) = rx.recv() {
                    let flags: EXECUTION_STATE = if prevent {
                        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
                    } else {
                        ES_CONTINUOUS
                    };
                    let result = unsafe { SetThreadExecutionState(flags) };
                    if result == 0 {
                        eprintln!("⚠️  SetThreadExecutionState failed");
                    }
                }
            })
            .expect("Failed to spawn sleep-guard thread");
        tx
    })
}

/// Prevents the system from entering sleep mode.
/// All calls are funneled to a single dedicated thread so that
/// SetThreadExecutionState (which is thread-local) works correctly.
pub fn prevent_sleep(prevent: bool) {
    let tx = get_sleep_sender();
    if let Err(e) = tx.send(prevent) {
        eprintln!("⚠️  Failed to send sleep command: {}", e);
        return;
    }
    
    let was = SLEEP_PREVENTED.swap(prevent, Ordering::SeqCst);
    
    if was != prevent {
        if prevent {
            println!("🔋 System sleep prevention ENABLED");
        } else {
            println!("🔋 System sleep prevention DISABLED");
        }
    }
}

/// Returns Some(percentage) if on battery power, None if plugged in or unknown.
pub fn get_battery_percentage() -> Option<u8> {
    unsafe {
        let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut status) != 0 {
            // ACLineStatus == 0 means Off-line (on battery)
            // ACLineStatus == 1 means On-line (plugged in)
            if status.ACLineStatus == 0 && status.BatteryLifePercent <= 100 {
                return Some(status.BatteryLifePercent);
            }
        }
    }
    None
}
