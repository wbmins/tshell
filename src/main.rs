#![windows_subsystem = "windows"]

use gpui::KeyBinding;

mod app;
mod backend;
mod session;
mod sftp;
mod sync;
mod system;
mod terminal;

rust_i18n::i18n!("locales", fallback = "en");

gpui::actions!(ashell_terminal, [TerminalTabKey, TerminalBacktabKey]);

pub(crate) use app::keybinding_recorder::{
    ClosePane, Copy, FocusPaneDown, FocusPaneLeft, FocusPaneRight, FocusPaneUp, NewSsh, OpenSearch,
    OpenSession, OpenSettings, OpenTransfers, Paste, SplitPaneDown, SplitPaneLeft, SplitPaneRight,
    SplitPaneUp, ToggleSftpZoom, ToggleSidebar,
};

pub(crate) use app::{Ashell, PaneLayout, SelectorEntry, SftpContextMenuState, TabGroup};

fn main() {
    let t0 = std::time::Instant::now();
    app::startup::sync_macos_launch_environment();
    let t1 = std::time::Instant::now();
    app::startup::init_logging();
    app::startup::startup_mark("sync_macos_launch_environment", t0, t1);

    #[cfg(target_os = "macos")]
    let app = gpui_platform::application()
        .with_assets(app::icons::Assets)
        .with_quit_mode(gpui::QuitMode::LastWindowClosed);

    #[cfg(not(target_os = "macos"))]
    let app = gpui_platform::application().with_assets(app::icons::Assets);
    app::startup::startup_mark("create application", t1, std::time::Instant::now());

    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            app::startup::open_main_window(cx);
        }
    });
    let t2 = std::time::Instant::now();
    app.run(move |cx| {
        let t3 = std::time::Instant::now();
        app::startup::startup_mark("gpui platform init (until run callback)", t2, t3);

        gpui_component::init(cx);
        let t4 = std::time::Instant::now();
        cx.bind_keys([
            KeyBinding::new(
                "tab",
                TerminalTabKey,
                Some(app::constants::TERMINAL_KEY_CONTEXT),
            ),
            KeyBinding::new(
                "shift-tab",
                TerminalBacktabKey,
                Some(app::constants::TERMINAL_KEY_CONTEXT),
            ),
        ]);
        app::startup::bind_workspace_keys(cx);
        let t5 = std::time::Instant::now();
        app::theme::load_embedded_themes(cx);
        let t6 = std::time::Instant::now();
        app::startup::open_main_window(cx);
        let t7 = std::time::Instant::now();
        app::startup::startup_mark("gpui_component::init", t3, t4);
        app::startup::startup_mark("bind_workspace_keys", t4, t5);
        app::startup::startup_mark("load_embedded_themes", t5, t6);
        app::startup::startup_mark("open_main_window", t6, t7);
        app::startup::startup_mark("run callback total", t3, t7);
        app::startup::startup_mark("total startup", t0, t7);
    });
}
