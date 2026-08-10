use std::borrow::Cow;

use gpui::{App, AssetSource, IntoElement, RenderOnce, Result, SharedString, Window};
use gpui_component::IconNamed;

// Custom icons embedded with the app, in addition to gpui-component's built-ins.
// Each `<name>.svg` in `assets/custom-icons` becomes a PascalCase variant of
// `AshellIcon` and is served at the `icons/<name>.svg` asset path.
gpui_component::icon_named!(AshellIcon, "assets/custom-icons");

impl RenderOnce for AshellIcon {
    fn render(self, _: &mut Window, _cx: &mut App) -> impl IntoElement {
        let icon: gpui_component::Icon = self.into();
        icon
    }
}

const CUSTOM_ICON_PATHS: &[&str] = &[
    "icons/file-down.svg",
    "icons/file-up.svg",
    "icons/refresh-ccw.svg",
    "icons/trash-2.svg",
    "icons/debian-icon.svg",
    "icons/ubuntu-icon.svg",
    "icons/alpinelinux-icon.svg",
    "icons/android-icon.svg",
    "icons/archlinux-icon.svg",
    "icons/postmarketos-icon.svg",
];

/// Asset source serving the app's custom icons, falling back to
/// gpui-component's default icon assets.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if CUSTOM_ICON_PATHS.contains(&path) {
            let bytes: &'static [u8] = match path {
                "icons/file-down.svg" => {
                    include_bytes!("../../assets/custom-icons/file-down.svg")
                }
                "icons/file-up.svg" => include_bytes!("../../assets/custom-icons/file-up.svg"),
                "icons/refresh-ccw.svg" => {
                    include_bytes!("../../assets/custom-icons/refresh-ccw.svg")
                }
                "icons/trash-2.svg" => {
                    include_bytes!("../../assets/custom-icons/trash-2.svg")
                }
                "icons/debian-icon.svg" => {
                    include_bytes!("../../assets/custom-icons/debian-icon.svg")
                }
                "icons/ubuntu-icon.svg" => {
                    include_bytes!("../../assets/custom-icons/ubuntu-icon.svg")
                }
                "icons/alpinelinux-icon.svg" => {
                    include_bytes!("../../assets/custom-icons/alpinelinux-icon.svg")
                }
                "icons/android-icon.svg" => {
                    include_bytes!("../../assets/custom-icons/android-icon.svg")
                }
                "icons/archlinux-icon.svg" => {
                    include_bytes!("../../assets/custom-icons/archlinux-icon.svg")
                }
                "icons/postmarketos-icon.svg" => {
                    include_bytes!("../../assets/custom-icons/postmarketos-icon.svg")
                }
                _ => unreachable!(),
            };
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        let default_assets = gpui_component_assets::Assets;
        default_assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names: Vec<SharedString> = CUSTOM_ICON_PATHS
            .iter()
            .filter(|p| p.starts_with(path))
            .map(|p| (*p).into())
            .collect();
        let default_assets = gpui_component_assets::Assets;
        names.extend(default_assets.list(path)?);
        Ok(names)
    }
}
