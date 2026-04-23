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

            // Mark that peek-wrap is active (so shells can detect it)
            let peek_env = CString::new("PEEK_ACTIVE=1").unwrap();
            libc::putenv(peek_env.into_raw());

            // For fish: disable built-in completions for tools we manage
            // by wrapping them with complete --erase before launching
            let is_fish = shell.contains("fish");
            if is_fish {
                let shell_cstr = CString::new(shell).unwrap();
                let c_flag = CString::new("-C").unwrap();
                // Erase existing completions AND register dummy ones.
                // The dummy registration prevents fish from lazy-loading
                // system completions from /usr/share/fish/completions/.
                let init_cmd = CString::new(concat!(
                    "complete -e -c pnpm; complete -c pnpm -f; ",
                    "complete -e -c npm; complete -c npm -f; ",
                    "complete -e -c yarn; complete -c yarn -f; ",
                    "complete -e -c bun; complete -c bun -f; ",
                    "complete -e -c make; complete -c make -f; ",
                    "complete -e -c cargo; complete -c cargo -f",
                )).unwrap();
                let args = [
                    shell_cstr.as_ptr(),
                    c_flag.as_ptr(),
                    init_cmd.as_ptr(),
                    std::ptr::null(),
                ];
                libc::execvp(shell_cstr.as_ptr(), args.as_ptr());
            } else {
                let shell_cstr = CString::new(shell).unwrap();
                let login_arg = CString::new("-l").unwrap();
                let args = [shell_cstr.as_ptr(), login_arg.as_ptr(), std::ptr::null()];
                libc::execvp(shell_cstr.as_ptr(), args.as_ptr());
            }

            // If exec fails
            libc::_exit(1);
        }
    }

    // Parent: close slave
    unsafe {
        libc::close(slave);
    }

    Ok((master, pid))
}
