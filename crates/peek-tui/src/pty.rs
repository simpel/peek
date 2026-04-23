use std::ffi::CString;
use std::os::fd::RawFd;

use anyhow::{bail, Result};

/// Spawn a shell in a new PTY. Returns (master_fd, child_pid).
pub fn spawn_shell(shell: &str, cols: u16, rows: u16) -> Result<(RawFd, libc::pid_t)> {
    let mut master: RawFd = 0;
    let mut slave: RawFd = 0;

    // Create PTY pair
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let ret = unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), &ws as *const _ as *mut _) };
    if ret != 0 {
        bail!("openpty failed: {}", std::io::Error::last_os_error());
    }

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        bail!("fork failed: {}", std::io::Error::last_os_error());
    }

    if pid == 0 {
        // Child process
        unsafe {
            // Close master in child
            libc::close(master);

            // Create new session
            libc::setsid();

            // Set controlling terminal
            libc::ioctl(slave, libc::TIOCSCTTY as libc::c_ulong, 0);

            // Redirect stdio to slave
            libc::dup2(slave, libc::STDIN_FILENO);
            libc::dup2(slave, libc::STDOUT_FILENO);
            libc::dup2(slave, libc::STDERR_FILENO);

            if slave > 2 {
                libc::close(slave);
            }

            // Set TERM if not set
            let term = CString::new("TERM=xterm-256color").unwrap();
            libc::putenv(term.into_raw());

            // Exec the shell
            let shell_cstr = CString::new(shell).unwrap();
            let login_arg = CString::new("-l").unwrap(); // login shell
            let args = [shell_cstr.as_ptr(), login_arg.as_ptr(), std::ptr::null()];
            libc::execvp(shell_cstr.as_ptr(), args.as_ptr());

            // If exec fails
            libc::_exit(1);
        }
    }

    // Parent: close slave
    unsafe {
        libc::close(slave);
    }

    // Set master to non-blocking
    unsafe {
        let flags = libc::fcntl(master, libc::F_GETFL);
        libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    Ok((master, pid))
}
