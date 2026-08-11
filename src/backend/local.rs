use std::{
    io::{Read, Write},
    sync::{Arc, Condvar, Mutex, mpsc::{self, Sender}},
    thread,
};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::terminal::{BackendCommand, BackendEvent, BackendTx};

pub fn spawn_local_terminal(
    tab_id: String,
    cols: u16,
    rows: u16,
    events: Sender<BackendEvent>,
) -> Result<BackendTx> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(windows) {
            "powershell.exe".into()
        } else {
            "/bin/zsh".into()
        }
    });
    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("SHELL", shell.clone());
    spawn_pty_command(tab_id, cols, rows, events, cmd, "local shell", false)
}

/// Spawn a WSL terminal that drops directly into a specific installed distro.
///
/// WSL's first launch can be slow (cold start of the distro). We therefore
/// delay the "connected" signal until the WSL shell has gone idle (stopped
/// emitting output for a short while), so the UI connection-progress overlay
/// stays visible until the shell is actually ready for input.
pub fn spawn_wsl_terminal(
    tab_id: String,
    cols: u16,
    rows: u16,
    events: Sender<BackendEvent>,
    distro: String,
) -> Result<BackendTx> {
    // `wsl.exe -d <distro>` starts an interactive shell inside that distro.
    // `--cd ~` ensures we land in the user's home directory.
    let mut cmd = CommandBuilder::new("wsl.exe");
    cmd.arg("-d");
    cmd.arg(distro);
    cmd.arg("--cd");
    cmd.arg("~");
    spawn_pty_command(tab_id, cols, rows, events, cmd, "wsl shell", true)
}

fn spawn_pty_command(
    tab_id: String,
    cols: u16,
    rows: u16,
    events: Sender<BackendEvent>,
    mut cmd: CommandBuilder,
    status_text: &'static str,
    connected_on_idle: bool,
) -> Result<BackendTx> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("open local PTY")?;

    cmd.env(
        "TERM",
        std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into()),
    );
    cmd.env(
        "COLORTERM",
        std::env::var("COLORTERM").unwrap_or_else(|_| "truecolor".into()),
    );
    cmd.env("TERM_PROGRAM", "ashell");
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    if let Ok(lang) = std::env::var("LANG") {
        cmd.env("LANG", lang);
    } else {
        cmd.env("LANG", "en_US.UTF-8");
    }
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("spawn local shell")?;
    drop(pair.slave);

    let master = pair.master;
    let mut reader = master.try_clone_reader().context("clone PTY reader")?;
    let mut writer = master.take_writer().context("take PTY writer")?;
    let (cmd_tx, cmd_rx) = mpsc::channel::<BackendCommand>();

    // Shared `(last output time, has any output arrived)` used to detect when
    // the WSL shell has gone idle (i.e. is waiting for input), which is when
    // it's truly ready. We use a Condvar so the idle watcher is event-driven
    // (notified on new output) rather than polling in a loop.
    let output_state: Arc<(Mutex<(std::time::Instant, bool)>, Condvar)> =
        Arc::new((Mutex::new((std::time::Instant::now(), false)), Condvar::new()));

    let read_tab = tab_id.clone();
    let read_events = events.clone();
    let read_output_state = output_state.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    {
                        let (lock, cv) = &*read_output_state;
                        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                        guard.0 = std::time::Instant::now();
                        guard.1 = true;
                        cv.notify_all();
                    }
                    let _ = read_events.send(BackendEvent::Output {
                        tab_id: read_tab.clone(),
                        bytes: buf[..n].to_vec(),
                    });
                }
                Err(err) => {
                    let _ = read_events.send(BackendEvent::Closed {
                        tab_id: read_tab.clone(),
                        reason: format!("local read error: {err}"),
                    });
                    return;
                }
            }
        }
        let _ = read_events.send(BackendEvent::Closed {
            tab_id: read_tab,
            reason: "local shell closed".into(),
        });
    });

    // For WSL, report "connected" once the shell has gone idle: some output
    // has been produced and no new output has arrived for IDLE_MS. This keeps
    // the connection-progress overlay visible through the (potentially slow)
    // WSL cold start, until the shell prompt is actually ready.
    //
    // Event-driven: the watcher sleeps on the Condvar and is woken up either
    // by new output (which resets the idle timer) or by an IDLE_MS timeout
    // (which means the shell is idle and ready).
    if connected_on_idle {
        const IDLE_MS: u64 = 700;
        let idle_tab = tab_id.clone();
        let idle_events = events.clone();
        let idle_output_state = output_state.clone();
        thread::spawn(move || loop {
            let (lock, cv) = &*idle_output_state;
            let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            // Sleep until either new output arrives (notify) or we've waited a
            // full IDLE_MS with no output (timeout).
            let (guard, _timeout) = cv
                .wait_timeout(guard, std::time::Duration::from_millis(IDLE_MS))
                .unwrap_or_else(|poison| poison.into_inner());
            let (has_output, idle_ms) = (guard.1, guard.0.elapsed().as_millis());
            if has_output && idle_ms as u64 >= IDLE_MS {
                let _ = idle_events.send(BackendEvent::Connected {
                    tab_id: idle_tab.clone(),
                });
                return;
            }
            // New output arrived (or not yet any output): loop and keep waiting.
        });
    }

    let write_tab = tab_id.clone();
    let write_events = events.clone();
    thread::spawn(move || {
        loop {
            match cmd_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(command) => match command {
                    BackendCommand::Input(bytes) => {
                        if let Err(err) = writer.write_all(&bytes) {
                            let _ = write_events.send(BackendEvent::Closed {
                                tab_id: write_tab.clone(),
                                reason: format!("local write error: {err}"),
                            });
                            break;
                        }
                        let _ = writer.flush();
                    }
                    BackendCommand::Resize { cols, rows } => {
                        let _ = master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                    BackendCommand::Close => break,
                    BackendCommand::SampleMetrics => {}
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Ok(Some(status)) = child.try_wait() {
                        let _ = write_events.send(BackendEvent::Closed {
                            tab_id: write_tab,
                            reason: format!("local shell exited: {status}"),
                        });
                        return;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = child.kill();
    });

    let _ = events.send(BackendEvent::Status {
        tab_id: tab_id.clone(),
        text: format!("{status_text} ready"),
    });

    // For local terminals the PTY is up as soon as the child spawned, so we
    // report "connected" immediately. For WSL, "connected" is sent later from
    // the idle watcher thread once the WSL shell has gone idle (see above),
    // keeping the connection-progress overlay visible through the cold start.
    if !connected_on_idle {
        let _ = events.send(BackendEvent::Connected { tab_id });
    }

    Ok(BackendTx::Local(cmd_tx))
}
