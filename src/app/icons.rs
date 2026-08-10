use std::borrow::Cow;

use gpui::{AssetSource, IntoElement, Result, SharedString};
use gpui_component::IconNamed;

// Custom icons embedded with the app, in addition to gpui-component's built-ins.
// Each `<name>.svg` in `assets/custom-icons` becomes a PascalCase variant of
// `AshellIcon` and is served at the `icons/<name>.svg` asset path.
gpui_component::icon_named!(AshellIcon, "assets/custom-icons");

const CUSTOM_ICON_PATHS: &[&str] = &["icons/file-down.svg"];

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
