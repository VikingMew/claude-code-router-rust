use anyhow::Result;
use ccr_app_core::client_config::claude::{
    ActivationStatus, activate_ccr, check_activation_status, deactivate_ccr,
};
use ccr_app_core::client_config::codex::{
    CodexActivationStatus, activate_codex_ccr, check_codex_activation_status, deactivate_codex_ccr,
};
use ccr_app_core::status::{is_process_alive, pid_file_path, read_pid, write_pid};
use ccr_config::{default_config_path, load_config};
use clap::{Parser, Subcommand};
use std::process::Command;

#[derive(Parser)]
#[command(name = "ccr", about = "Claude Code Router")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start,
    Stop,
    Restart,
    Status,
    /// Backup Claude config and switch to CCR router
    ClaudeActivate,
    /// Restore original Claude config
    ClaudeDeactivate,
    /// Backup Codex config and switch to CCR router
    CodexActivate,
    /// Restore original Codex config
    CodexDeactivate,
    /// Manage presets
    Preset {
        #[command(subcommand)]
        action: PresetCommands,
    },
    /// Read JSON from stdin and output statusline string
    Statusline,
}

#[derive(Subcommand)]
enum PresetCommands {
    List,
    Info { name: String },
    Delete { name: String },
    Export { name: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let pid_path = pid_file_path();

    match cli.command {
        Commands::Start => {
            if let Some(pid) = read_pid(&pid_path) {
                if is_process_alive(pid) {
                    println!("Already running (PID {pid})");
                    return Ok(());
                }
            }
            let exe = std::env::current_exe()?
                .parent()
                .unwrap()
                .join("ccr-server");
            let child = Command::new(&exe).spawn()?;
            let pid = child.id();
            write_pid(&pid_path, pid)?;
            println!("Server started (PID {pid})");
        }

        Commands::Stop => {
            let pid = read_pid(&pid_path).ok_or_else(|| anyhow::anyhow!("Not running"))?;
            #[cfg(unix)]
            {
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;
                signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM)?;
            }
            std::fs::remove_file(&pid_path).ok();
            println!("Server stopped.");
        }

        Commands::Restart => {
            if let Some(pid) = read_pid(&pid_path) {
                if is_process_alive(pid) {
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{self, Signal};
                        use nix::unistd::Pid;
                        signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM).ok();
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
            let exe = std::env::current_exe()?
                .parent()
                .unwrap()
                .join("ccr-server");
            let child = Command::new(&exe).spawn()?;
            let pid = child.id();
            write_pid(&pid_path, pid)?;
            println!("Server restarted (PID {pid})");
        }

        Commands::Status => {
            match read_pid(&pid_path) {
                Some(pid) if is_process_alive(pid) => {
                    let config = load_config(&default_config_path()).unwrap_or_default();
                    let port = config.port.unwrap_or(3456);
                    println!("CCR Server: Running (PID {pid}, port {port})");
                }
                _ => println!("CCR Server: Not running"),
            }

            // Show Claude config activation status
            println!();
            match check_activation_status() {
                ActivationStatus::Activated {
                    backup_path,
                    backup_time,
                } => {
                    println!("Claude Config: Using CCR router");
                    if let Some(time) = backup_time {
                        println!("Backup: {} ({})", backup_path.display(), time);
                    } else {
                        println!("Backup: {}", backup_path.display());
                    }
                }
                ActivationStatus::Deactivated => {
                    println!("Claude Config: Using original configuration");
                }
            }

            println!();
            match check_codex_activation_status() {
                CodexActivationStatus::Activated {
                    backup_path,
                    backup_time,
                } => {
                    println!("Codex Config: Using CCR router");
                    if let Some(time) = backup_time {
                        println!("Backup: {} ({})", backup_path.display(), time);
                    } else {
                        println!("Backup: {}", backup_path.display());
                    }
                }
                CodexActivationStatus::Deactivated => {
                    println!("Codex Config: Using original configuration");
                }
            }
        }

        Commands::ClaudeActivate => {
            if let Err(e) = activate_ccr() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::ClaudeDeactivate => {
            if let Err(e) = deactivate_ccr() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::CodexActivate => {
            if let Err(e) = activate_codex_ccr() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::CodexDeactivate => {
            if let Err(e) = deactivate_codex_ccr() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Preset { action } => match action {
            PresetCommands::List => {
                let presets = ccr_preset::list_presets().unwrap_or_default();
                if presets.is_empty() {
                    println!("No presets installed.");
                } else {
                    for p in &presets {
                        println!("{} ({})", p.name, p.version);
                    }
                }
            }
            PresetCommands::Info { name } => {
                let dir = ccr_preset::presets_dir().join(&name);
                match ccr_preset::load_preset(dir.to_str().unwrap_or("")) {
                    Ok(m) => println!("{}", serde_json::to_string_pretty(&m)?),
                    Err(_) => println!("Preset '{name}' not found."),
                }
            }
            PresetCommands::Delete { name } => match ccr_preset::delete_preset(&name) {
                Ok(_) => println!("Deleted preset '{name}'."),
                Err(e) => println!("Error: {e}"),
            },
            PresetCommands::Export { name } => {
                let config = load_config(&default_config_path()).unwrap_or_default();
                match ccr_preset::export_preset(&name, &config) {
                    Ok(path) => println!("Exported to {}", path.display()),
                    Err(e) => println!("Error: {e}"),
                }
            }
        },

        Commands::Statusline => {
            let mut input = String::new();
            std::io::stdin().lines().for_each(|l| {
                if let Ok(line) = l {
                    input.push_str(&line);
                }
            });
            let v: serde_json::Value =
                serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
            let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("unknown");
            let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("idle");
            println!("CCR [{model}] {status}");
        }
    }
    Ok(())
}
