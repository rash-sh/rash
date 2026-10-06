//! Termination signal handling for Rash and the processes it supervises.
//!
//! Synchronous children run in Rash's own process group, like the foreground job of a
//! non-interactive shell: they can read and configure the controlling terminal, and
//! terminal-generated signals (Ctrl-C) reach them directly. While such a child runs,
//! Rash only records termination signals (forwarding user-sent ones such as `docker stop`
//! to the child); once the child is reaped the signal becomes an
//! [`ErrorKind::Interrupted`](crate::error::ErrorKind::Interrupted) error that aborts the
//! script but still runs `always` sections.
//!
//! When no child is being supervised (between tasks, while polling async jobs, inside
//! in-process modules), a termination signal kills every async job process group and
//! exits immediately with `128 + signal`.
use crate::error::{Error, ErrorKind, Result};

use std::io;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

const HANDLED_SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

/// No supervised child: a termination signal exits Rash immediately.
const NO_CHILD: i32 = 0;
/// Supervising a child whose pid is unknown or already exited: only record signals.
const BUSY: i32 = -1;

/// Upper bound of concurrently tracked async job process groups.
const MAX_JOB_GROUPS: usize = 1024;

static FOREGROUND_CHILD: AtomicI32 = AtomicI32::new(NO_CHILD);
static PENDING_SIGNAL: AtomicI32 = AtomicI32::new(0);
static PENDING_FROM_TERMINAL: AtomicBool = AtomicBool::new(false);
/// Signal the supervised child still has to receive. Whoever swaps it out sends it, so
/// the handler and [`ForegroundGuard::attach`] racing on another thread deliver it once.
static UNFORWARDED_SIGNAL: AtomicI32 = AtomicI32::new(0);
static JOB_GROUPS: [AtomicI32; MAX_JOB_GROUPS] = [const { AtomicI32::new(0) }; MAX_JOB_GROUPS];

/// Install Rash's termination handlers for SIGINT, SIGTERM and SIGHUP.
///
/// Signals ignored at startup (e.g. SIGHUP under `nohup`, SIGINT for background jobs of a
/// non-interactive shell) stay ignored.
pub fn install_handlers() -> Result<()> {
    for signal in HANDLED_SIGNALS {
        // SAFETY: an all-zero sigaction is valid output storage for sigaction(2).
        let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
        // SAFETY: a null new action only queries the current disposition.
        if unsafe { libc::sigaction(signal, std::ptr::null(), &mut previous) } != 0 {
            return Err(Error::new(ErrorKind::Other, io::Error::last_os_error()));
        }
        if previous.sa_sigaction == libc::SIG_IGN {
            continue;
        }

        // SAFETY: an all-zero sigaction is a valid starting point; fields are set below.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = handle_signal as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        // SAFETY: sa_mask is owned storage; the handler only uses async-signal-safe calls
        // (atomics, kill(2), _exit(2)).
        let installed = unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut())
        };
        if installed != 0 {
            return Err(Error::new(ErrorKind::Other, io::Error::last_os_error()));
        }
    }
    Ok(())
}

extern "C" fn handle_signal(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
    let child = FOREGROUND_CHILD.load(Ordering::SeqCst);
    if child == NO_CHILD {
        kill_job_groups();
        // SAFETY: _exit(2) is async-signal-safe.
        unsafe { libc::_exit(128 + signal) };
    }
    // si_code <= 0 means kill(2)/sigqueue(3) from a process; positive codes (SI_KERNEL)
    // come from the kernel, e.g. the terminal driver for Ctrl-C or hangup.
    // SAFETY: with SA_SIGINFO the kernel passes a valid siginfo_t pointer.
    let from_user = info.is_null() || unsafe { (*info).si_code } <= 0;
    PENDING_FROM_TERMINAL.store(!from_user, Ordering::SeqCst);
    PENDING_SIGNAL.store(signal, Ordering::SeqCst);
    // Terminal signals already reached an attached child through the shared process
    // group; a child attached later missed them.
    if from_user || child == BUSY {
        UNFORWARDED_SIGNAL.store(signal, Ordering::SeqCst);
        // Reload: `attach` may have published the pid after the load above and checked
        // for unforwarded signals before the store above.
        forward_unforwarded(FOREGROUND_CHILD.load(Ordering::SeqCst));
    }
}

/// Send the unforwarded signal, if any, to `child`. Async-signal-safe.
fn forward_unforwarded(child: i32) {
    if child <= 0 {
        return;
    }
    let signal = UNFORWARDED_SIGNAL.swap(0, Ordering::SeqCst);
    if signal != 0 {
        // SAFETY: kill(2) is async-signal-safe; the pid is not reaped while attached.
        unsafe { libc::kill(child, signal) };
    }
}

/// Marks a synchronous child as supervised by Rash for its lifetime.
///
/// Create it before spawning the child so that signals arriving during the spawn are
/// recorded and forwarded on [`attach`](Self::attach) instead of terminating Rash.
#[derive(Debug)]
pub struct ForegroundGuard(());

impl ForegroundGuard {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        UNFORWARDED_SIGNAL.store(0, Ordering::SeqCst);
        FOREGROUND_CHILD.store(BUSY, Ordering::SeqCst);
        Self(())
    }

    /// Forward user-sent termination signals to `pid` from now on.
    pub fn attach(&self, pid: u32) {
        let Ok(pid) = i32::try_from(pid) else {
            return;
        };
        FOREGROUND_CHILD.store(pid, Ordering::SeqCst);
        // Deliver a signal received while the child was being spawned.
        forward_unforwarded(pid);
    }

    /// Stop forwarding: call after the child exited and before it is reaped.
    pub fn detach(&self) {
        FOREGROUND_CHILD.store(BUSY, Ordering::SeqCst);
    }

    /// Consume a signal received while supervising the child.
    ///
    /// Like a shell foreground job, a terminal Ctrl-C that the child handled itself
    /// (it exited normally, e.g. an editor or REPL) does not interrupt Rash.
    pub fn take_interrupt(&self, child_killed_by_signal: bool) -> Option<Error> {
        let signal = PENDING_SIGNAL.swap(0, Ordering::SeqCst);
        let from_terminal = PENDING_FROM_TERMINAL.swap(false, Ordering::SeqCst);
        (signal != 0 && interrupt_applies(signal, from_terminal, child_killed_by_signal))
            .then(|| Error::interrupted(signal))
    }
}

impl Drop for ForegroundGuard {
    fn drop(&mut self) {
        FOREGROUND_CHILD.store(NO_CHILD, Ordering::SeqCst);
        UNFORWARDED_SIGNAL.store(0, Ordering::SeqCst);
    }
}

fn interrupt_applies(signal: i32, from_terminal: bool, child_killed_by_signal: bool) -> bool {
    !(signal == libc::SIGINT && from_terminal && !child_killed_by_signal)
}

/// Whether a termination signal is waiting to be turned into an interrupt error.
pub fn interrupt_pending() -> bool {
    PENDING_SIGNAL.load(Ordering::SeqCst) != 0
}

/// Consume a recorded signal that no supervised child turned into an error yet.
pub fn take_pending_interrupt() -> Option<Error> {
    PENDING_FROM_TERMINAL.store(false, Ordering::SeqCst);
    match PENDING_SIGNAL.swap(0, Ordering::SeqCst) {
        0 => None,
        signal => Some(Error::interrupted(signal)),
    }
}

/// Block until `pid` exits without reaping it, so its pid cannot be recycled while the
/// signal handler may still forward to it.
pub fn wait_exited(pid: u32) -> io::Result<()> {
    loop {
        // SAFETY: an all-zero siginfo_t is valid output storage for waitid(2).
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: WNOWAIT leaves the child waitable; the caller reaps it afterwards.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                libc::id_t::from(pid),
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// Track an async job process group so termination signals kill it.
pub fn register_job_group(pgid: u32) {
    let Ok(pgid) = i32::try_from(pgid) else {
        return;
    };
    let registered = JOB_GROUPS.iter().any(|slot| {
        slot.compare_exchange(0, pgid, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    });
    if !registered {
        warn!("Too many async jobs: process group {pgid} is not killed on interrupt");
    }
}

pub fn unregister_job_group(pgid: u32) {
    let Ok(pgid) = i32::try_from(pgid) else {
        return;
    };
    for slot in &JOB_GROUPS {
        let _ = slot.compare_exchange(pgid, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// SIGKILL every tracked async job process group. Async-signal-safe.
pub fn kill_job_groups() {
    for slot in &JOB_GROUPS {
        let pgid = slot.swap(0, Ordering::SeqCst);
        if pgid > 0 {
            // SAFETY: kill(2) is async-signal-safe; a negative pid addresses a process group.
            unsafe { libc::kill(-pgid, libc::SIGKILL) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_sigint_handled_by_child_does_not_interrupt() {
        assert!(!interrupt_applies(libc::SIGINT, true, false));
        assert!(interrupt_applies(libc::SIGINT, true, true));
    }

    #[test]
    fn user_sent_and_term_signals_always_interrupt() {
        assert!(interrupt_applies(libc::SIGINT, false, false));
        assert!(interrupt_applies(libc::SIGTERM, false, false));
        assert!(interrupt_applies(libc::SIGTERM, true, false));
        assert!(interrupt_applies(libc::SIGHUP, true, false));
    }

    #[test]
    fn wait_exited_leaves_child_reapable() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        wait_exited(child.id()).unwrap();
        assert!(child.wait().unwrap().success());
    }
}
