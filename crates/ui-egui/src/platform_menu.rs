//! Menu items a platform shell adds to the app's menus (the Android app's Shortcut Pad under
//! Window), dispatched back to the shell. Like plug-in rows, they are built each time the menus
//! are, so their checkmarks follow the shell's state.

use crate::PhotocraftApp;
use crate::menus::MenuItem;
use serde_json::Value;

/// Builds the shell's rows as they are now.
pub type ItemsFn = Box<dyn Fn() -> Vec<MenuItem>>;
/// Runs a picked row.
pub type RunFn = Box<dyn FnMut(&str) -> Result<Value, String>>;

/// The shell's items and their handler. Ids are the shell's own (conventionally prefixed with
/// the platform, `android.`); `path` places each row (`["Window", "Shortcut Pad"]` is a
/// submenu of Window).
pub struct PlatformMenu {
    /// The rows as they are now (label, path, enabled, checked).
    pub items: ItemsFn,
    /// Runs a picked row.
    pub run: RunFn,
}

fn items(app: &PhotocraftApp) -> Vec<MenuItem> {
    app.services.platform_menu.as_ref().map(|m| (m.items)()).unwrap_or_default()
}

/// Add the shell's rows after the last row of their top-level menu (before Window's document
/// list, which is appended later).
pub(crate) fn insert_menu_items(app: &PhotocraftApp, out: &mut Vec<MenuItem>) {
    for item in items(app) {
        let top = item.path.first().cloned();
        let at = out.iter().rposition(|i| i.path.first() == top.as_ref()).map_or(out.len(), |p| p + 1);
        out.insert(at, item);
    }
}

/// Run `id` when it is one of the shell's rows.
pub(crate) fn menu(app: &mut PhotocraftApp, id: &str) -> Option<Result<Value, String>> {
    let item = items(app).into_iter().find(|i| i.id == id)?;
    if !item.enabled {
        return Some(Err(format!("{} is not available now", item.label)));
    }
    let menu = app.services.platform_menu.as_mut()?;
    Some((menu.run)(id))
}

/// Whether `id` is enabled, when it is one of the shell's rows.
pub(crate) fn is_enabled(app: &PhotocraftApp, id: &str) -> Option<bool> {
    items(app).into_iter().find(|i| i.id == id).map(|i| i.enabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn row(id: &str, path: &[&str], checked: Option<bool>, enabled: bool) -> MenuItem {
        MenuItem {
            id: id.into(),
            label: id.rsplit('.').next().unwrap_or(id).into(),
            path: path.iter().map(|s| s.to_string()).collect(),
            shortcut: None,
            enabled,
            checked,
            color: None,
        }
    }

    #[test]
    fn platform_rows_appear_in_their_menu_follow_state_and_run() {
        let shown = Rc::new(RefCell::new(false));
        let ran = Rc::new(RefCell::new(Vec::<String>::new()));
        let (s1, s2, r) = (shown.clone(), shown.clone(), ran.clone());
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 32, "height": 32})).unwrap();
        app.services.platform_menu = Some(PlatformMenu {
            items: Box::new(move || {
                vec![
                    row("android.pad.show", &["Window", "Shortcut Pad"], Some(*s1.borrow()), true),
                    row("android.pad.off", &["Window", "Shortcut Pad"], None, false),
                ]
            }),
            run: Box::new(move |id| {
                r.borrow_mut().push(id.to_string());
                *s2.borrow_mut() = true;
                Ok(Value::Null)
            }),
        });
        let items = crate::menus::menu_items(&app);
        let show = items.iter().position(|i| i.id == "android.pad.show").expect("listed");
        assert_eq!(items[show].path, ["Window", "Shortcut Pad"]);
        assert_eq!(items[show].checked, Some(false));
        let doc = items.iter().position(|i| i.id.starts_with(crate::menus::DOCUMENT_ITEM)).expect("document row");
        assert!(show < doc, "Window's document list stays last");

        let ctx = egui::Context::default();
        crate::menus::invoke(&mut app, &ctx, "android.pad.show", json!({})).unwrap();
        assert_eq!(*ran.borrow(), ["android.pad.show"]);
        let items = crate::menus::menu_items(&app);
        assert_eq!(items.iter().find(|i| i.id == "android.pad.show").and_then(|i| i.checked), Some(true), "checkmarks follow the shell");

        assert!(crate::menus::is_enabled(&app, "android.pad.show"));
        assert!(!crate::menus::is_enabled(&app, "android.pad.off"));
        assert!(crate::menus::invoke(&mut app, &ctx, "android.pad.off", json!({})).is_err(), "a disabled row doesn't run");
        assert_eq!(ran.borrow().len(), 1);
    }

    #[test]
    fn without_a_platform_menu_nothing_changes() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        assert!(!crate::menus::menu_items(&app).iter().any(|i| i.id.starts_with("android.")));
        assert_eq!(is_enabled(&app, "android.pad.show"), None);
    }
}
