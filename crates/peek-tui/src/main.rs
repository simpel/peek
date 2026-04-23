mod dropdown;
mod line_tracker;
mod pty;

use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use crossterm::terminal;

use crate::dropdown::Dropdown;
use crate::line_tracker::LineTracker;

fn main() -> Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    // Get current terminal size
    let (cols, rows) = terminal::size().context("failed to get terminal size")?;

    // Create PTY and spawn shell
    let (master_fd, child_pid) = pty::spawn_shell(&shell, cols, rows)?;
    let master = unsafe { OwnedFd::from_raw_fd(master_fd) };

    // Put our terminal into raw mode
    terminal::enable_raw_mode().context("failed to enable raw mode")?;

    let result = run_event_loop(&master, cols, rows);

    // Restore terminal
    terminal::disable_raw_mode().ok();

    // Wait for child
    unsafe {
        let mut status: libc::c_int = 0;
        libc::waitpid(child_pid, &mut status, 0);
    }

    result
}

fn run_event_loop(master: &OwnedFd, _cols: u16, rows: u16) -> Result<()> {
    let master_fd = master.as_raw_fd();

    let dropdown = Arc::new(Mutex::new(Dropdown::new()));
    let line_tracker = Arc::new(Mutex::new(LineTracker::new()));
    let terminal_rows = Arc::new(Mutex::new(rows));

    // Thread 1: read from PTY master → write to stdout (shell output)
    let dropdown_clone = dropdown.clone();
    let line_tracker_clone = line_tracker.clone();
    let pty_reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdout = io::stdout();
        loop {
            let n = unsafe { libc::read(master_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if n <= 0 {
                break;
            }
            let data = &buf[..n as usize];

            // Clear dropdown before shell output overwrites it
            {
                let mut dd = dropdown_clone.lock().unwrap();
                if dd.visible {
                    dd.clear(&mut stdout);
                }
            }

            stdout.write_all(data).ok();
            stdout.flush().ok();

            // After shell output, re-render dropdown if still relevant
            {
                let tracker = line_tracker_clone.lock().unwrap();
                let mut dd = dropdown_clone.lock().unwrap();
                if dd.visible && !tracker.current_line().is_empty() {
                    dd.render(&mut stdout);
                }
            }
        }
    });

    // Thread 2: read from stdin → process or forward to PTY master (user input)
    let dropdown_clone = dropdown.clone();
    let line_tracker_clone = line_tracker.clone();
    let _stdin_reader = std::thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut buf = [0u8; 64];

        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            let data = &buf[..n];

            let mut dd = dropdown_clone.lock().unwrap();
            let mut tracker = line_tracker_clone.lock().unwrap();

            if dd.visible {
                // Check for navigation keys
                match data {
                    // Up arrow: ESC [ A
                    [0x1b, b'[', b'A'] => {
                        dd.move_up();
                        dd.clear(&mut stdout);
                        dd.render(&mut stdout);
                        stdout.flush().ok();
                        continue;
                    }
                    // Down arrow: ESC [ B
                    [0x1b, b'[', b'B'] => {
                        dd.move_down();
                        dd.clear(&mut stdout);
                        dd.render(&mut stdout);
                        stdout.flush().ok();
                        continue;
                    }
                    // Tab: accept selection
                    [0x09] => {
                        if let Some(selected) = dd.selected_name() {
                            let selected = selected.to_string();
                            dd.clear(&mut stdout);
                            dd.hide();

                            // Delete the current filter text and replace with selection
                            let filter = tracker.filter_text();
                            // Send backspaces to delete filter text
                            for _ in 0..filter.len() {
                                let bs = [0x7f]; // DEL
                                unsafe {
                                    libc::write(
                                        master_fd,
                                        bs.as_ptr() as *const _,
                                        bs.len(),
                                    );
                                }
                            }
                            // Send the selected name
                            unsafe {
                                libc::write(
                                    master_fd,
                                    selected.as_ptr() as *const _,
                                    selected.len(),
                                );
                            }
                            tracker.replace_filter(&selected);
                            stdout.flush().ok();
                            continue;
                        }
                    }
                    // Escape: dismiss
                    [0x1b] => {
                        dd.clear(&mut stdout);
                        dd.hide();
                        stdout.flush().ok();
                        continue;
                    }
                    // Enter: accept and submit
                    [0x0d] => {
                        if let Some(selected) = dd.selected_name() {
                            let selected = selected.to_string();
                            dd.clear(&mut stdout);
                            dd.hide();

                            // Delete filter and type selection + enter
                            let filter = tracker.filter_text();
                            for _ in 0..filter.len() {
                                let bs = [0x7f];
                                unsafe {
                                    libc::write(
                                        master_fd,
                                        bs.as_ptr() as *const _,
                                        bs.len(),
                                    );
                                }
                            }
                            unsafe {
                                libc::write(
                                    master_fd,
                                    selected.as_ptr() as *const _,
                                    selected.len(),
                                );
                            }
                            // Send enter
                            let enter = [0x0d];
                            unsafe {
                                libc::write(
                                    master_fd,
                                    enter.as_ptr() as *const _,
                                    enter.len(),
                                );
                            }
                            tracker.reset();
                            stdout.flush().ok();
                            continue;
                        }
                    }
                    _ => {}
                }
            }

            // Forward input to shell
            unsafe {
                libc::write(master_fd, data.as_ptr() as *const _, data.len());
            }

            // Update line tracker
            tracker.feed(data);

            // Check if we should show/update dropdown
            let line = tracker.current_line();
            drop(tracker); // release lock before querying daemon
            drop(dd);

            if let Some((_tool, _filter)) = peek_core::tools::match_tool_prefix(&line) {
                // Query the daemon
                if let Ok(suggestions) = query_daemon(&line) {
                    let mut dd = dropdown_clone.lock().unwrap();
                    if !suggestions.is_empty() {
                        dd.update(suggestions);
                        dd.clear(&mut stdout);
                        dd.render(&mut stdout);
                    } else {
                        dd.clear(&mut stdout);
                        dd.hide();
                    }
                    stdout.flush().ok();
                }
            } else {
                let mut dd = dropdown_clone.lock().unwrap();
                if dd.visible {
                    dd.clear(&mut stdout);
                    dd.hide();
                    stdout.flush().ok();
                }
            }
        }
    });

    // Handle SIGWINCH (terminal resize)
    let master_raw = master.as_raw_fd();
    let rows_clone = terminal_rows.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if let Ok((new_cols, new_rows)) = terminal::size() {
                let mut rows = rows_clone.lock().unwrap();
                if *rows != new_rows {
                    *rows = new_rows;
                    // Forward resize to PTY
                    let ws = libc::winsize {
                        ws_row: new_rows,
                        ws_col: new_cols,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    };
                    unsafe {
                        libc::ioctl(master_raw, libc::TIOCSWINSZ, &ws);
                    }
                }
            }
        }
    });

    pty_reader.join().ok();
    // stdin_reader will end when shell exits

    Ok(())
}


fn query_daemon(line: &str) -> Result<Vec<(String, String)>> {
    let cwd = std::env::current_dir()?.to_string_lossy().to_string();

    let socket_path = dirs_socket_path();
    if !socket_path.exists() {
        return Ok(vec![]);
    }

    let mut stream = std::os::unix::net::UnixStream::connect(&socket_path)?;
    let request = peek_core::protocol::Request::Suggest {
        cwd,
        line: line.to_string(),
        cursor: line.len(),
    };
    let mut json = serde_json::to_string(&request)?;
    json.push('\n');
    stream.write_all(json.as_bytes())?;

    let mut reader = std::io::BufReader::new(stream);
    let mut response_line = String::new();
    std::io::BufRead::read_line(&mut reader, &mut response_line)?;

    let response: peek_core::protocol::Response = serde_json::from_str(response_line.trim())?;
    match response {
        peek_core::protocol::Response::Suggestions { suggestions, .. } => {
            Ok(suggestions
                .into_iter()
                .map(|s| (s.name, s.preview))
                .collect())
        }
        _ => Ok(vec![]),
    }
}

fn dirs_socket_path() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".peek")
        .join("peek.sock")
}
