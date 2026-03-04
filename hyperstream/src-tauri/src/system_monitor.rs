use sysinfo::{System, ProcessesToUpdate};
use crate::speed_limiter::GLOBAL_LIMITER;
use std::time::Duration;
use tokio::time::sleep;

const GAME_PROCESSES: &[&str] = &[
    "csgo.exe",
    "dota2.exe",
    "valorant.exe",
    "destiny2.exe",
    "league of legends.exe",
    "overwatch.exe",
    "fortnite.exe",
    "apex.exe",
    "cod.exe",
    "r6.exe",
];

pub async fn run_game_mode_monitor() {
    let mut system = System::new_all();
    let mut game_mode_active = false;
    let mut previous_limit = 0;

    println!("Starting Game Mode Monitor...");

    loop {
        sleep(Duration::from_secs(5)).await;
        
        // Refresh processes
        system.refresh_processes(ProcessesToUpdate::All, true);

        let active_game = system.processes().values().any(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            GAME_PROCESSES.iter().any(|g| name == *g)
        });

        if active_game {
            if !game_mode_active {
                let current_limit = GLOBAL_LIMITER.get_limit();
                // Only throttle if current limit is higher than game mode limit (or unlimited)
                if current_limit == 0 || current_limit > 500 * 1024 {
                    println!("[GameMode] Game detected! Throttling download speed.");
                    previous_limit = current_limit;
                    GLOBAL_LIMITER.set_limit(500 * 1024); // 500 KB/s
                    game_mode_active = true;
                }
            }
        } else {
            if game_mode_active {
                // Only restore if the limit hasn't been changed by the user during gameplay
                let current_limit = GLOBAL_LIMITER.get_limit();
                if current_limit == 500 * 1024 {
                    println!("[GameMode] Game closed. Restoring speed limit.");
                    GLOBAL_LIMITER.set_limit(previous_limit);
                }
                game_mode_active = false;
            }
        }
    }
}
