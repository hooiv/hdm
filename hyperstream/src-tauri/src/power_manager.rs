use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, SetThreadExecutionState, SYSTEM_POWER_STATUS,
    ES_CONTINUOUS, ES_SYSTEM_REQUIRED, EXECUTION_STATE
};

static SLEEP_PREVENTED: AtomicBool = AtomicBool::new(false);

/// Prevents the system from entering sleep mode
pub fn prevent_sleep(prevent: bool) {
    let current = SLEEP_PREVENTED.load(Ordering::SeqCst);
    if current == prevent { return; }

    unsafe {
        let flags: EXECUTION_STATE = if prevent {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        };
        SetThreadExecutionState(flags);
    }
    SLEEP_PREVENTED.store(prevent, Ordering::SeqCst);
    
    if prevent {
        println!("🔋 System sleep prevention ENABLED");
    } else {
        println!("🔋 System sleep prevention DISABLED");
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
