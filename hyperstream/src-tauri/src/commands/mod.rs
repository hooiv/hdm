//! Command handlers for Tauri IPC
//!
//! All Tauri commands are organized here and exported for registration in generate_handler!

pub mod settings_cmds;
pub mod queue_manager_cmds;
pub mod resilience_cmds;
pub mod state_management_cmds;
pub mod segment_integrity_cmds;
pub mod mirror_scoring_cmds;
pub mod download_groups_cmds;
pub mod advanced_group_cmds;
pub mod group_metrics_cmds;
pub mod queue_orchestrator_commands;
pub mod preflight_commands;
pub mod download_history_commands;

