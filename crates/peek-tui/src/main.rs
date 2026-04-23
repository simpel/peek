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

    let (cols, rows) = terminal::size().context("failed to get terminal size")?;

    let (master_fd, child_pid) = pty::spawn_shell(&shell, cols, rows)?;

    // Put terminal into raw mode
    terminal::enable_raw_mode().context("failed to enable raw mode")?;

    let result = run_event_loop(master_fd, rows);

    terminal::disable_raw_mode().ok();

    // Wait for child
    unsafe {
        let mut status: libc::c_int = 0;
        libc::waitpid(child_pid, &mut status, 0);
    }

    result
}

fn run_event_loop(master_fd: libc::c_int, rows: u16) -> Result<()> {
    let dropdown = Arc::new(Mutex::new(Dropdown::new()));
    let line_tracker = Arc::new(Mutex::new(LineTracker::new()));

    let stdin_fd = io::stdin().as_raw_fd();

    // Thread: read from PTY master → write to stdout
    let dropdown_out = dropdown.clone();
    let master_fd_copy = master_fd;
    let pty_reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdout = io::stdout();
        loop {
            let n = unsafe { libc::read(master_fd_copy, buf.as_mut_ptr() as *mut _, buf.len()) };
            if n <= 0 {
                break;
            }
            let data = &buf[..n as usize];

            // Clear dropdown before writing shell output
            {
                let mut dd = dropdown_out.lock().unwrap();
                if dd.visible {
                    dd.clear(&mut stdout);
                }
            }

            stdout.write_all(data).ok();
            stdout.flush().ok();

            // Re-render dropdown after shell output
            {
                let mut dd = dropdown_out.lock().unwrap();
                if dd.visible {
                    dd.render(&mut stdout);
                    stdout.flush().ok();
                }
            }
        }
    });

    // Thread: handle terminal resize
    let master_fd_copy = master_fd;
    let current_rows = Arc::new(Mutex::new(rows));
    let rows_ref = current_rows.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Ok((new_cols, new_rows)) = terminal::size() {
            let mut r = rows_ref.lock().unwrap();
            if *r != new_rows {
                *r = new_rows;
                let ws = libc::winsize {
                    ws_row: new_rows,
                    ws_col: new_cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                unsafe {
                    libc::ioctl(master_fd_copy, libc::TIOCSWINSZ, &ws);
                }
            }
        }
    });

    // Main thread: read stdin → process or forward to PTY
    let mut stdout = io::stdout();
    let mut seq_buf: Vec<u8> = Vec::new();

    loop {
        // Use poll to wait for stdin data
        let mut fds = [libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        }];

        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 100) };
        if ret <= 0 {
            // Check if shell is still alive
            let mut status: libc::c_int = 0;
            let w = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if w > 0 {
                break; // Shell exited
            }
            continue;
        }

        let mut buf = [0u8; 256];
        let n = unsafe { libc::read(stdin_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n <= 0 {
            break;
        }
        let data = &buf[..n as usize];

        let mut dd = dropdown.lock().unwrap();
        let mut tracker = line_tracker.lock().unwrap();

        if dd.visible {
            // Check for navigation sequences in the input
            let mut i = 0;
            while i < data.len() {
                let remaining = &data[i..];
                match remaining {
                    // Up arrow: ESC [ A
                    [0x1b, b'[', b'A', ..] => {
                        dd.move_up();
                        dd.clear(&mut stdout);
                        dd.render(&mut stdout);
                        stdout.flush().ok();
                        i += 3;
                    }
                    // Down arrow: ESC [ B
                    [0x1b, b'[', b'B', ..] => {
                        dd.move_down();
                        dd.clear(&mut stdout);
                        dd.render(&mut stdout);
                        stdout.flush().ok();
                        i += 3;
                    }
                    // Tab: accept selection
                    [0x09, ..] => {
                        if let Some(selected) = dd.selected_name() {
                            let selected = selected.to_string();
                            dd.clear(&mut stdout);
                            dd.hide();
                            stdout.flush().ok();

                            // Delete filter text and type the selection
                            let filter = tracker.filter_text();
                            for _ in 0..filter.len() {
                                let bs = [0x08]; // BS
                                unsafe {
                                    libc::write(master_fd, bs.as_ptr() as *const _, 1);
                                }
                            }
                            unsafe {
                                libc::write(
                                    master_fd,
                                    selected.as_ptr() as *const _,
                                    selected.len(),
                                );
                            }
                            tracker.replace_filter(&selected);
                        }
                        i += 1;
                    }
                    // Escape (standalone): dismiss dropdown
                    [0x1b] if remaining.len() == 1 => {
                        dd.clear(&mut stdout);
                        dd.hide();
                        stdout.flush().ok();
                        i += 1;
                    }
                    // Enter: accept and run
                    [0x0d, ..] => {
                        if let Some(selected) = dd.selected_name() {
                            let selected = selected.to_string();
                            dd.clear(&mut stdout);
                            dd.hide();
                            stdout.flush().ok();

                            let filter = tracker.filter_text();
                            for _ in 0..filter.len() {
                                let bs = [0x08];
                                unsafe {
                                    libc::write(master_fd, bs.as_ptr() as *const _, 1);
                                }
                            }
                            unsafe {
                                libc::write(
                                    master_fd,
                                    selected.as_ptr() as *const _,
                                    selected.len(),
                                );
                                let enter = [0x0du8];
                                libc::write(master_fd, enter.as_ptr() as *const _, 1);
                            }
                            tracker.reset();
                        } else {
                            // No selection, just forward enter
                            unsafe {
                                libc::write(master_fd, data[i..i + 1].as_ptr() as *const _, 1);
                            }
                            tracker.reset();
                        }
                        i += 1;
                    }
                    // Any other byte: forward to shell and update tracker
                    _ => {
                        unsafe {
                            libc::write(master_fd, data[i..i + 1].as_ptr() as *const _, 1);
                        }
                        tracker.feed(&data[i..i + 1]);
                        i += 1;

                        // Update dropdown with new filter
                        let line = tracker.current_line();
                        if let Some((_tool, _filter)) = peek_core::tools::match_tool_prefix(&line)
                        {
                            drop(tracker);
                            drop(dd);
                            if let Ok(suggestions) = query_daemon(&line) {
                                let mut dd = dropdown.lock().unwrap();
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
                            // Re-acquire locks for next iteration
                            dd = dropdown.lock().unwrap();
                            tracker = line_tracker.lock().unwrap();
                        } else {
                            dd.clear(&mut stdout);
                            dd.hide();
                            stdout.flush().ok();
                        }
                    }
                }
            }
        } else {
            // No dropdown visible: forward everything to shell, track input
            unsafe {
                libc::write(master_fd, data.as_ptr() as *const _, data.len());
            }
            tracker.feed(data);

            let line = tracker.current_line();
            if let Some((_tool, _filter)) = peek_core::tools::match_tool_prefix(&line) {
                drop(tracker);
                drop(dd);
                if let Ok(suggestions) = query_daemon(&line) {
                    let mut dd = dropdown.lock().unwrap();
                    if !suggestions.is_empty() {
                        dd.update(suggestions);
                        dd.render(&mut stdout);
                        stdout.flush().ok();
                    }
                }
            }
        }
    }

    pty_reader.join().ok();
    Ok(())
}

fn query_daemon(line: &str) -> Result<Vec<(String, String)>> {
    let cwd = std::env::current_dir()?.to_string_lossy().to_string();

    let socket_path = dirs::home_dir()
        .expect("no home dir")
        .join(".peek")
        .join("peek.sock");

    if !socket_path.exists() {
        return Ok(vec![]);
    }

    let mut stream = std::os::unix::net::UnixStream::connect(&socket_path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_millis(100)))?;

    let request = peek_core::protocol::Request::Suggest {
        cwd,
        line: line.to_string(),
        cursor: line.len(),
    };
    let mut json = serde_json::to_string(&request)?;
    json.push('\n');
    stream.write_all(json.as_bytes())?;

    let mut reader = io::BufReader::new(stream);
    let mut response_line = String::new();
    io::BufRead::read_line(&mut reader, &mut response_line)?;

    let response: peek_core::protocol::Response = serde_json::from_str(response_line.trim())?;
    match response {
        peek_core::protocol::Response::Suggestions { suggestions, .. } => Ok(suggestions
            .into_iter()
            .map(|s| (s.name, s.preview))
            .collect()),
        _ => Ok(vec![]),
    }
}
