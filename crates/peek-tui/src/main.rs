mod line_tracker;
mod overlay;
mod pty;

use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use crossterm::terminal;

use crate::line_tracker::LineTracker;
use crate::overlay::OverlayProcess;

fn main() -> Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    let (cols, rows) = terminal::size().context("failed to get terminal size")?;
    let (master_fd, child_pid) = pty::spawn_shell(&shell, cols, rows)?;

    // Spawn the native overlay GUI process
    let overlay = Arc::new(Mutex::new(OverlayProcess::spawn()?));

    terminal::enable_raw_mode().context("failed to enable raw mode")?;

    let result = run_event_loop(master_fd, &overlay);

    terminal::disable_raw_mode().ok();

    // Kill overlay
    if let Ok(mut o) = overlay.lock() {
        o.kill();
    }

    // Wait for shell child
    unsafe {
        let mut status: libc::c_int = 0;
        libc::waitpid(child_pid, &mut status, 0);
    }

    result
}

struct DropdownState {
    visible: bool,
    items: Vec<(String, String)>,
    selected: usize,
}

impl DropdownState {
    fn new() -> Self {
        Self {
            visible: false,
            items: Vec::new(),
            selected: 0,
        }
    }
}

fn run_event_loop(master_fd: libc::c_int, overlay: &Arc<Mutex<OverlayProcess>>) -> Result<()> {
    let line_tracker = Arc::new(Mutex::new(LineTracker::new()));
    let dd_state = Arc::new(Mutex::new(DropdownState::new()));

    let stdin_fd = io::stdin().as_raw_fd();

    // Thread: read from PTY master → write to stdout (pass-through)
    let master_fd_copy = master_fd;
    let pty_reader = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdout = io::stdout();
        loop {
            let n = unsafe { libc::read(master_fd_copy, buf.as_mut_ptr() as *mut _, buf.len()) };
            if n <= 0 {
                break;
            }
            stdout.write_all(&buf[..n as usize]).ok();
            stdout.flush().ok();
        }
    });

    // Thread: handle terminal resize
    let master_fd_copy = master_fd;
    std::thread::spawn(move || {
        let mut last_size = (0u16, 0u16);
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if let Ok(size) = terminal::size() {
                if size != last_size {
                    last_size = size;
                    let ws = libc::winsize {
                        ws_row: size.1,
                        ws_col: size.0,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    };
                    unsafe {
                        libc::ioctl(master_fd_copy, libc::TIOCSWINSZ, &ws);
                    }
                }
            }
        }
    });

    // Main thread: read stdin → process or forward to PTY
    loop {
        let mut fds = [libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        }];

        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 100) };
        if ret <= 0 {
            let mut status: libc::c_int = 0;
            let w = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if w > 0 {
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

        let mut dd = dd_state.lock().unwrap();
        let mut tracker = line_tracker.lock().unwrap();

        if dd.visible {
            let mut i = 0;
            while i < data.len() {
                let remaining = &data[i..];
                match remaining {
                    // Up arrow
                    [0x1b, b'[', b'A', ..] => {
                        if dd.selected > 0 {
                            dd.selected -= 1;
                        } else {
                            dd.selected = dd.items.len().saturating_sub(1);
                        }
                        if let Ok(mut o) = overlay.lock() {
                            o.update_selection(dd.selected);
                        }
                        i += 3;
                    }
                    // Down arrow
                    [0x1b, b'[', b'B', ..] => {
                        if dd.selected < dd.items.len().saturating_sub(1) {
                            dd.selected += 1;
                        } else {
                            dd.selected = 0;
                        }
                        if let Ok(mut o) = overlay.lock() {
                            o.update_selection(dd.selected);
                        }
                        i += 3;
                    }
                    // Tab: accept
                    [0x09, ..] => {
                        if let Some((name, _)) = dd.items.get(dd.selected) {
                            let name = name.clone();
                            let filter = tracker.filter_text();
                            // Delete filter, type selection
                            for _ in 0..filter.len() {
                                unsafe { libc::write(master_fd, b"\x08".as_ptr() as *const _, 1); }
                            }
                            unsafe {
                                libc::write(master_fd, name.as_ptr() as *const _, name.len());
                            }
                            tracker.replace_filter(&name);
                        }
                        dd.visible = false;
                        if let Ok(mut o) = overlay.lock() { o.hide(); }
                        i += 1;
                    }
                    // Escape (standalone)
                    [0x1b] if remaining.len() == 1 => {
                        dd.visible = false;
                        if let Ok(mut o) = overlay.lock() { o.hide(); }
                        i += 1;
                    }
                    // Enter: accept and run
                    [0x0d, ..] => {
                        if let Some((name, _)) = dd.items.get(dd.selected) {
                            let name = name.clone();
                            let filter = tracker.filter_text();
                            for _ in 0..filter.len() {
                                unsafe { libc::write(master_fd, b"\x08".as_ptr() as *const _, 1); }
                            }
                            unsafe {
                                libc::write(master_fd, name.as_ptr() as *const _, name.len());
                                libc::write(master_fd, b"\r".as_ptr() as *const _, 1);
                            }
                        } else {
                            unsafe { libc::write(master_fd, b"\r".as_ptr() as *const _, 1); }
                        }
                        dd.visible = false;
                        if let Ok(mut o) = overlay.lock() { o.hide(); }
                        tracker.reset();
                        i += 1;
                    }
                    // Other: forward and update
                    _ => {
                        unsafe { libc::write(master_fd, data[i..i+1].as_ptr() as *const _, 1); }
                        tracker.feed(&data[i..i+1]);
                        i += 1;

                        let line = tracker.current_line();
                        if peek_core::tools::match_tool_prefix(&line).is_some() {
                            // Re-query daemon
                            drop(tracker);
                            drop(dd);
                            update_dropdown(&line, &dd_state, overlay);
                            dd = dd_state.lock().unwrap();
                            tracker = line_tracker.lock().unwrap();
                        } else {
                            dd.visible = false;
                            if let Ok(mut o) = overlay.lock() { o.hide(); }
                        }
                    }
                }
            }
        } else {
            // Forward everything, track input
            unsafe {
                libc::write(master_fd, data.as_ptr() as *const _, data.len());
            }
            tracker.feed(data);

            let line = tracker.current_line();
            if peek_core::tools::match_tool_prefix(&line).is_some() {
                drop(tracker);
                drop(dd);
                update_dropdown(&line, &dd_state, overlay);
            }
        }
    }

    pty_reader.join().ok();
    Ok(())
}

fn update_dropdown(
    line: &str,
    dd_state: &Arc<Mutex<DropdownState>>,
    overlay: &Arc<Mutex<OverlayProcess>>,
) {
    if let Ok(suggestions) = query_daemon(line) {
        let mut dd = dd_state.lock().unwrap();
        if !suggestions.is_empty() {
            dd.items = suggestions.clone();
            dd.selected = 0;
            dd.visible = true;
            let pos = query_overlay_position(line.len());
            if let Ok(mut o) = overlay.lock() {
                o.show(&suggestions, 0, pos);
            }
        } else {
            dd.visible = false;
            if let Ok(mut o) = overlay.lock() { o.hide(); }
        }
    }
}

/// Read a terminal response from stdin, terminated by the given byte.
fn read_terminal_response(stdin_fd: libc::c_int, terminator: u8) -> Option<String> {
    let mut buf = [0u8; 64];
    let mut pos = 0;

    for _ in 0..64 {
        let mut fds = [libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        }];
        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 100) };
        if ret <= 0 {
            break;
        }
        let n = unsafe { libc::read(stdin_fd, buf[pos..pos + 1].as_mut_ptr() as *mut _, 1) };
        if n <= 0 {
            break;
        }
        pos += 1;
        if buf[pos - 1] == terminator {
            break;
        }
    }
    if pos > 0 {
        std::str::from_utf8(&buf[..pos]).ok().map(|s| s.to_string())
    } else {
        None
    }
}

/// Query terminal geometry and cursor position.
/// Returns (screen_x, screen_y) pixel coordinates for the dropdown.
fn query_overlay_position(line_len: usize) -> Option<(i32, i32)> {
    use std::io::Write;
    let stdin_fd = io::stdin().as_raw_fd();
    let mut stdout = io::stdout();

    // Query cursor position: \e[6n → \e[row;colR
    stdout.write_all(b"\x1b[6n").ok()?;
    stdout.flush().ok()?;
    let cursor_resp = read_terminal_response(stdin_fd, b'R')?;
    let cursor_inner = cursor_resp.strip_prefix("\x1b[")?.strip_suffix('R')?;
    let (row_str, col_str) = cursor_inner.split_once(';')?;
    let cursor_row: i32 = row_str.parse().ok()?;
    let cursor_col: i32 = col_str.parse().ok()?;

    // Query window position: \e[13t → \e[3;x;yt
    stdout.write_all(b"\x1b[13t").ok()?;
    stdout.flush().ok()?;
    let win_pos_resp = read_terminal_response(stdin_fd, b't')?;
    let win_inner = win_pos_resp.strip_prefix("\x1b[3;")?.strip_suffix('t')?;
    let (wx_str, wy_str) = win_inner.split_once(';')?;
    let win_x: i32 = wx_str.parse().ok()?;
    let win_y: i32 = wy_str.parse().ok()?;

    // Query window size in pixels: \e[14t → \e[4;height;widtht
    stdout.write_all(b"\x1b[14t").ok()?;
    stdout.flush().ok()?;
    let win_size_resp = read_terminal_response(stdin_fd, b't')?;
    let size_inner = win_size_resp.strip_prefix("\x1b[4;")?.strip_suffix('t')?;
    let (wh_str, ww_str) = size_inner.split_once(';')?;
    let win_pixel_h: i32 = wh_str.parse().ok()?;
    let win_pixel_w: i32 = ww_str.parse().ok()?;

    // Get terminal dimensions in characters
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((80, 24));

    // Calculate cell size
    let cell_h = win_pixel_h / term_rows as i32;
    let cell_w = win_pixel_w / term_cols as i32;

    // Screen coordinates for dropdown (just below the cursor line)
    let x = win_x + (cursor_col - 1) * cell_w;
    let y = win_y + cursor_row * cell_h; // row is 1-based, so cursor_row * cell_h = below the cursor line

    Some((x, y))
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
