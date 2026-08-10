use gpui::{App, AppContext as _, Bounds, WindowOptions, point, px, size};
use gpui_component::Root;

use crate::Ashell;
use crate::session::config::ConfigStore;

pub(crate) fn startup_mark(label: &str, start: std::time::Instant, end: std::time::Instant) {
    tracing::info!(
        "[startup] {label}: {:.1} ms",
        end.duration_since(start).as_secs_f64() * 1000.0
    );
}

pub(crate) fn bind_workspace_keys(cx: &mut gpui::App) {
    let config = ConfigStore::load().unwrap_or_else(|_| ConfigStore::in_memory());
    crate::app::keybinding_recorder::bind_workspace_keys_from_config(cx, &config);
}

pub(crate) fn init_logging() {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let log_file = directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".config").join("ashell").join("log"))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ashell.log");

    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let file_appender = tracing_appender::rolling::never(
        log_file.parent().unwrap_or(std::path::Path::new(".")),
        "ashell.log",
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    // Leak the guard so it lives for the entire duration of the app since GPUI's run might not return
    std::mem::forget(_guard);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let file_layer = tracing_subscriber::fmt::layer()
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();
    tracing::info!("[startup] logging initialized, log file: {}", log_file.display());
}

#[cfg(target_os = "macos")]
pub(crate) fn sync_macos_launch_environment() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let Ok(output) = std::process::Command::new(&shell)
        .args(["-l", "-c", "env -0"])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }

    for entry in output.stdout.split(|b| *b == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|b| *b == b'=') else {
            continue;
        };
        let Ok(key) = std::str::from_utf8(&entry[..eq]) else {
            continue;
        };
        let Ok(value) = std::str::from_utf8(&entry[eq + 1..]) else {
            continue;
        };

        let should_import = matches!(
            key,
            "PATH"
                | "MANPATH"
                | "INFOPATH"
                | "LANG"
                | "LC_ALL"
                | "LC_CTYPE"
                | "SHELL"
                | "HOME"
                | "HOMEBREW_PREFIX"
                | "HOMEBREW_CELLAR"
                | "HOMEBREW_REPOSITORY"
                | "HTTP_PROXY"
                | "HTTPS_PROXY"
                | "ALL_PROXY"
                | "http_proxy"
                | "https_proxy"
                | "all_proxy"
        ) || key.starts_with("LC_");

        if should_import {
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }
}

fn read_proxy_from_env() -> Option<(String, String, Option<u16>, String, String)> {
    let vars = [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ];
    for var in vars {
        if let Ok(val) = std::env::var(var) {
            if val.is_empty() {
                continue;
            }
            if let Ok(url) = reqwest::Url::parse(&val) {
                let scheme = url.scheme();
                let proxy_type = match scheme {
                    "socks5" | "socks5h" => "socks5".to_string(),
                    "http" | "https" => "http".to_string(),
                    _ => "socks5".to_string(),
                };
                let host = url.host_str().unwrap_or("").to_string();
                let port = url.port();
                let user = url.username().to_string();
                let password = url.password().unwrap_or("").to_string();
                return Some((proxy_type, host, port, user, password));
            }
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn sync_macos_launch_environment() {}

pub(crate) fn open_main_window(cx: &mut App) {
    let t0 = std::time::Instant::now();
    let config = ConfigStore::load().unwrap_or_else(|_| ConfigStore::in_memory());
    let t1 = std::time::Instant::now();
    startup_mark("open_main_window: load config", t0, t1);

    let _ = crate::session::config::ENV_PROXY.get_or_init(|| {
        read_proxy_from_env().map(|(proxy_type, host, port, user, password)| {
            tracing::info!(
                "[proxy] Loaded proxy configuration from environment: type={}, host={}, port={:?}, user={}",
                proxy_type,
                host,
                port,
                user
            );
            crate::session::config::EnvProxy {
                proxy_type,
                host,
                port,
                user,
                pass: password,
            }
        })
    });
    let t2 = std::time::Instant::now();
    startup_mark("open_main_window: proxy env", t1, t2);

    let mut window_options = WindowOptions::default();

    if config.title_bar_style() == crate::session::config::TitleBarStyle::Integrated {
        window_options.titlebar = Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(9.0), px(9.0))),
        });
    }

    #[cfg(not(target_os = "macos"))]
    if let Ok(img) = image::load_from_memory(include_bytes!("../../assets/icons/ashell.png")) {
        window_options.icon = Some(std::sync::Arc::new(img.into_rgba8()));
    }

    if let Some(bounds) = config.window_bounds() {
        window_options.window_bounds = Some(match bounds {
            crate::session::config::SavedWindowBounds::Fullscreen {
                x,
                y,
                width,
                height,
            } => gpui::WindowBounds::Fullscreen(Bounds::new(
                point(px(*x), px(*y)),
                size(px(*width), px(*height)),
            )),
            crate::session::config::SavedWindowBounds::Maximized {
                x,
                y,
                width,
                height,
            } => gpui::WindowBounds::Maximized(Bounds::new(
                point(px(*x), px(*y)),
                size(px(*width), px(*height)),
            )),
            crate::session::config::SavedWindowBounds::Windowed {
                x,
                y,
                width,
                height,
            } => gpui::WindowBounds::Windowed(Bounds::new(
                point(px(*x), px(*y)),
                size(px(*width), px(*height)),
            )),
        });
    } else if let Some(display) = cx.displays().first().cloned() {
        let display_bounds = display.bounds();
        let width = display_bounds.size.width * 0.8;
        let height = display_bounds.size.height * 0.9;

        let x = display_bounds.origin.x + (display_bounds.size.width - width) / 2.0;

        #[cfg(target_os = "macos")]
        let y = display_bounds.origin.y;
        #[cfg(not(target_os = "macos"))]
        let y = display_bounds.origin.y + (display_bounds.size.height - height) / 2.0;

        window_options.window_bounds = Some(gpui::WindowBounds::Windowed(Bounds::new(
            point(x, y),
            size(width, height),
        )));
    }
    let t3 = std::time::Instant::now();
    startup_mark("open_main_window: window options", t2, t3);

    cx.open_window(window_options, |window, cx| {
        let win_t0 = std::time::Instant::now();
        window.activate_window();
        window.set_window_title("ashell");
        gpui_component::Theme::sync_system_appearance(Some(window), cx);
        let view = cx.new(|cx| Ashell::new(window, cx));
        let win_t1 = std::time::Instant::now();
        startup_mark("open_window: Ashell::new", win_t0, win_t1);

        tracing::info!("[ui] main application window opened");
        let focus_handle = view.read(cx).focus_handle.clone();
        window.focus(&focus_handle, cx);

        let view_clone = view.clone();
        window.on_window_should_close(cx, move |window: &mut gpui::Window, cx: &mut gpui::App| {
            let handle = window.window_handle();
            if !cx.windows().contains(&handle) {
                tracing::warn!(
                    "[ui] window not found in app during close, skipping save layout state."
                );
                return true;
            }
            view_clone.read(cx).save_layout_state(window, cx);
            true
        });

        let root = cx.new(|cx| Root::new(view, window, cx));
        startup_mark("open_window: Root::new", win_t1, std::time::Instant::now());
        startup_mark("open_window: callback total", win_t0, std::time::Instant::now());
        root
    })
    .expect("failed to open window");
    startup_mark(
        "open_main_window: open_window (total)",
        t3,
        std::time::Instant::now(),
    );
}
