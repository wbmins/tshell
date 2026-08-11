use std::{
    io::{Read, Write},
    sync::mpsc::{self, Sender},
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
/// report "connected" only once the WSL shell emits its first output, so the
/// UI keeps showing the connection progress overlay until WSL is actually
/// ready.
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
    connected_on_output: bool,
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

    let read_tab = tab_id.clone();
    let read_events = events.clone();
    let notify_connected = connected_on_output;
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        // When `notify_connected` is set (WSL), we report "connected" only on
        // the first chunk of output so the connection progress overlay stays
        // visible during a slow WSL cold start.
        let mut connected_sent = !notify_connected;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if !connected_sent {
                        connected_sent = true;
                        let _ = read_events.send(BackendEvent::Connected {
                            tab_id: read_tab.clone(),
                        });
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
    // the reader thread once the WSL shell emits its first output (see above),
    // keeping the connection-progress overlay visible during a cold start.
    if !connected_on_output {
        let _ = events.send(BackendEvent::Connected { tab_id });
    }

    Ok(BackendTx::Local(cmd_tx))
}
