mod line_tracker;
mod pty;
mod tui;

use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use crossterm::terminal;

use crate::line_tracker::LineTracker;
use crate::tui::TuiDropdown;

fn main() -> Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let (cols, rows) = terminal::size().context("failed to get terminal size")?;
    let (master_fd, child_pid) = pty::spawn_shell(&shell, cols, rows)?;

    terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let result = run_event_loop(master_fd);
    terminal::disable_raw_mode().ok();

    unsafe {
        let mut status: libc::c_int = 0;
        libc::waitpid(child_pid, &mut status, 0);
    }

    result
}

fn run_event_loop(master_fd: libc::c_int) -> Result<()> {
    let dd = Arc::new(Mutex::new(TuiDropdown::new()));
    let tracker = Arc::new(Mutex::new(LineTracker::new()));
    let stdin_fd = io::stdin().as_raw_fd();

    // Thread: PTY output → stdout (pass-through, but pause for dropdown redraws)
    let dd_out = dd.clone();
    let pty_reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdout = io::stdout();
        loop {
            let n = unsafe { libc::read(master_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if n <= 0 {
                break;
            }
            let data = &buf[..n as usize];

            let mut d = dd_out.lock().unwrap();
            if d.visible {
                d.clear(&mut stdout);
            }

            stdout.write_all(data).ok();
            stdout.flush().ok();

            if d.visible {
                d.render(&mut stdout);
            }
        }
    });

    // Thread: terminal resize → PTY
    let mfd = master_fd;
    std::thread::spawn(move || {
        let mut last = (0u16, 0u16);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if let Ok(size) = terminal::size() {
                if size != last {
                    last = size;
                    let ws = libc::winsize {
                        ws_row: size.1,
                        ws_col: size.0,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    };
                    unsafe { libc::ioctl(mfd, libc::TIOCSWINSZ, &ws); }
                }
            }
        }
    });

    // Main loop: stdin → process or forward to PTY
    let mut stdout = io::stdout();

    loop {
        let mut fds = [libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        }];

        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 100) };
        if ret <= 0 {
            let mut status: libc::c_int = 0;
            if unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) } > 0 {
                break;
            }
            continue;
        }

        let mut buf = [0u8; 256];
        let n = unsafe { libc::read(stdin_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n <= 0 {
            break;
        }
        let data = &buf[..n as usize];

        let mut d = dd.lock().unwrap();
        let mut t = tracker.lock().unwrap();

        if d.visible {
            let mut i = 0;
            while i < data.len() {
                let rest = &data[i..];
                match rest {
                    // Up arrow
                    [0x1b, b'[', b'A', ..] => {
                        d.move_up();
                        d.clear(&mut stdout);
                        d.render(&mut stdout);
                        i += 3;
                    }
                    // Down arrow
                    [0x1b, b'[', b'B', ..] => {
                        d.move_down();
                        d.clear(&mut stdout);
                        d.render(&mut stdout);
                        i += 3;
                    }
                    // Tab: accept selection
                    [0x09, ..] => {
                        if let Some(name) = d.selected_name() {
                            let name = name.to_string();
                            d.clear(&mut stdout);
                            d.hide();
                            stdout.flush().ok();

                            let filter = t.filter_text();
                            send_backspaces(master_fd, filter.len());
                            send_text(master_fd, &name);
                            t.replace_filter(&name);
                        }
                        i += 1;
                    }
                    // Escape
                    [0x1b] if rest.len() == 1 => {
                        d.clear(&mut stdout);
                        d.hide();
                        stdout.flush().ok();
                        i += 1;
                    }
                    // Enter: accept and run
                    [0x0d, ..] => {
                        if let Some(name) = d.selected_name() {
                            let name = name.to_string();
                            d.clear(&mut stdout);
                            d.hide();
                            stdout.flush().ok();

                            let filter = t.filter_text();
                            send_backspaces(master_fd, filter.len());
                            send_text(master_fd, &name);
                            send_byte(master_fd, 0x0d);
                        } else {
                            send_byte(master_fd, 0x0d);
                        }
                        t.reset();
                        i += 1;
                    }
                    // Other keys: forward + update suggestions
                    _ => {
                        send_bytes(master_fd, &data[i..i + 1]);
                        t.feed(&data[i..i + 1]);
                        i += 1;

                        let line = t.current_line();
                        if peek_core::tools::match_tool_prefix(&line).is_some() {
                            drop(t);
                            drop(d);
                            refresh_suggestions(&line, &dd, &mut stdout);
                            d = dd.lock().unwrap();
                            t = tracker.lock().unwrap();
                        } else {
                            d.clear(&mut stdout);
                            d.hide();
                            stdout.flush().ok();
                        }
                    }
                }
            }
        } else {
            // No dropdown: forward all input, track typing
            send_bytes(master_fd, data);
            t.feed(data);

            let line = t.current_line();
            if peek_core::tools::match_tool_prefix(&line).is_some() {
                drop(t);
                drop(d);
                refresh_suggestions(&line, &dd, &mut stdout);
            }
        }
    }

    pty_reader.join().ok();
    Ok(())
}

fn refresh_suggestions(line: &str, dd: &Arc<Mutex<TuiDropdown>>, stdout: &mut io::Stdout) {
    if let Ok(suggestions) = query_daemon(line) {
        let mut d = dd.lock().unwrap();
        if !suggestions.is_empty() {
            d.clear(stdout);
            d.update(suggestions);
            d.render(stdout);
        } else {
            d.clear(stdout);
            d.hide();
            stdout.flush().ok();
        }
    }
}

fn send_bytes(fd: libc::c_int, data: &[u8]) {
    unsafe { libc::write(fd, data.as_ptr() as *const _, data.len()); }
}

fn send_byte(fd: libc::c_int, b: u8) {
    send_bytes(fd, &[b]);
}

fn send_text(fd: libc::c_int, text: &str) {
    send_bytes(fd, text.as_bytes());
}

fn send_backspaces(fd: libc::c_int, count: usize) {
    for _ in 0..count {
        send_byte(fd, 0x08);
    }
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
