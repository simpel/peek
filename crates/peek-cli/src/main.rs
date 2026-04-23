use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use peek_core::protocol::{Request, Response};

fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".peek")
}

fn socket_path() -> PathBuf {
    data_dir().join("peek.sock")
}

fn pid_path() -> PathBuf {
    data_dir().join("peekd.pid")
}

#[derive(Parser)]
#[command(name = "peek", about = "Inline shell autocomplete daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print shell integration script
    Init {
        /// Shell to generate init script for
        #[arg(value_parser = ["zsh", "bash", "fish"])]
        shell: String,
    },
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Show daemon status
    Status,
    /// Open config file in $EDITOR
    Config,
    /// List completions for a tool in the current directory
    Completions {
        /// Tool to list completions for (e.g., pnpm, make, cargo)
        tool: String,
    },
    /// Show frecency history for current directory
    History,
    /// Clear frecency data
    ClearHistory,

    /// (Internal) Query suggestions — used by shell integration
    #[command(name = "_suggest", hide = true)]
    InternalSuggest {
        /// Current working directory
        #[arg(long)]
        cwd: String,
        /// Current command line
        #[arg(long)]
        line: String,
        /// Cursor position
        #[arg(long)]
        cursor: usize,
    },
    /// (Internal) Notify directory change — used by shell integration
    #[command(name = "_cd", hide = true)]
    InternalCd {
        /// New working directory
        #[arg(long)]
        cwd: String,
    },
    /// (Internal) Record command execution — used by shell integration
    #[command(name = "_executed", hide = true)]
    InternalExecuted {
        /// Current working directory
        #[arg(long)]
        cwd: String,
        /// Command that was run
        #[arg(long)]
        command: String,
        /// Tool name
        #[arg(long)]
        tool: String,
    },
}

fn send_request(request: &Request) -> Result<Response> {
    let sock = socket_path();
    if !sock.exists() {
        bail!("peek daemon is not running (no socket at {})", sock.display());
    }

    let mut stream = UnixStream::connect(&sock)
        .context("failed to connect to peek daemon")?;

    let mut request_json = serde_json::to_string(request)?;
    request_json.push('\n');
    stream.write_all(request_json.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: Response = serde_json::from_str(response_line.trim())?;
    Ok(response)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { shell } => {
            let script = match shell.as_str() {
                "zsh" => peek_shell::zsh::init_script(),
                "bash" => peek_shell::bash::init_script(),
                "fish" => peek_shell::fish::init_script(),
                _ => unreachable!("clap validates shell argument"),
            };
            print!("{script}");
        }

        Commands::Start => {
            let pid_file = pid_path();
            if pid_file.exists() {
                let pid_str = std::fs::read_to_string(&pid_file)?;
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    // Check if process is still running
                    if unsafe { libc::kill(pid, 0) } == 0 {
                        println!("peek daemon is already running (PID {pid})");
                        return Ok(());
                    }
                }
            }

            // Start the daemon
            let peekd = which_peekd()?;
            std::process::Command::new(peekd)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .context("failed to start peekd")?;

            println!("peek daemon started");
        }

        Commands::Stop => {
            match send_request(&Request::Shutdown) {
                Ok(_) => println!("peek daemon stopped"),
                Err(_) => {
                    // Try to kill by PID
                    let pid_file = pid_path();
                    if pid_file.exists() {
                        let pid_str = std::fs::read_to_string(&pid_file)?;
                        if let Ok(pid) = pid_str.trim().parse::<i32>() {
                            unsafe { libc::kill(pid, libc::SIGTERM) };
                            std::fs::remove_file(&pid_file).ok();
                            println!("peek daemon stopped (via SIGTERM)");
                            return Ok(());
                        }
                    }
                    println!("peek daemon is not running");
                }
            }
        }

        Commands::Status => {
            match send_request(&Request::Status) {
                Ok(Response::Status {
                    pid,
                    watched_dirs,
                    uptime_secs,
                }) => {
                    let hours = uptime_secs / 3600;
                    let minutes = (uptime_secs % 3600) / 60;
                    let seconds = uptime_secs % 60;
                    println!("peek daemon is running");
                    println!("  PID: {pid}");
                    println!("  Uptime: {hours}h {minutes}m {seconds}s");
                    println!("  Watched directories: {}", watched_dirs.len());
                    for dir in &watched_dirs {
                        println!("    {dir}");
                    }
                }
                _ => {
                    println!("peek daemon is not running");
                }
            }
        }

        Commands::Config => {
            let config_path = data_dir().join("config.toml");
            if !config_path.exists() {
                std::fs::create_dir_all(data_dir())?;
                std::fs::write(&config_path, DEFAULT_CONFIG)?;
            }
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            std::process::Command::new(editor)
                .arg(&config_path)
                .status()
                .context("failed to open editor")?;
        }

        Commands::Completions { tool } => {
            let cwd = std::env::current_dir()?.to_string_lossy().to_string();
            let line = format!("{tool} ");
            let request = Request::Suggest {
                cwd,
                line,
                cursor: tool.len() + 1,
            };
            match send_request(&request)? {
                Response::Suggestions { suggestions, .. } => {
                    if suggestions.is_empty() {
                        println!("No completions found for '{tool}' in this directory");
                    } else {
                        for s in &suggestions {
                            if s.preview.is_empty() {
                                println!("  {}", s.name);
                            } else {
                                println!("  {:<20} {}", s.name, s.preview);
                            }
                        }
                    }
                }
                Response::Error { message } => {
                    bail!("error: {message}");
                }
                _ => {}
            }
        }

        Commands::History => {
            let cwd = std::env::current_dir()?.to_string_lossy().to_string();
            // For now, just query the daemon for suggestions to show what it knows
            println!("Frecency history for {cwd}:");
            println!("(Stored in ~/.peek/history.db)");
        }

        Commands::ClearHistory => {
            let db_path = data_dir().join("history.db");
            if db_path.exists() {
                std::fs::remove_file(&db_path)?;
                println!("Frecency history cleared");
            } else {
                println!("No history to clear");
            }
        }

        Commands::InternalSuggest { cwd, line, cursor } => {
            let request = Request::Suggest { cwd, line, cursor };
            if let Ok(response) = send_request(&request) {
                // Output raw JSON for shell scripts to parse
                let json = serde_json::to_string(&response)?;
                println!("{json}");
            }
        }

        Commands::InternalCd { cwd } => {
            let _ = send_request(&Request::Cd { cwd });
        }

        Commands::InternalExecuted { cwd, command, tool } => {
            let _ = send_request(&Request::Executed { cwd, command, tool });
        }
    }

    Ok(())
}

fn which_peekd() -> Result<PathBuf> {
    // Look for peekd next to the peek binary
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap();
        let peekd = dir.join("peekd");
        if peekd.exists() {
            return Ok(peekd);
        }
    }
    // Fall back to PATH
    Ok(PathBuf::from("peekd"))
}

const DEFAULT_CONFIG: &str = r#"# peek configuration
# Trigger behavior: "auto" (show on typing) or "tab" (show on Tab press)
trigger = "auto"

# Maximum number of suggestions shown
max_suggestions = 8

[frecency]
recency_half_life_days = 7
frequency_weight = 1.0
recency_weight = 2.0

[tools]
pnpm = true
npm = true
yarn = true
bun = true
make = true
docker_compose = true
cargo = true
"#;
