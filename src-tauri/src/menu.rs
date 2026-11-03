//! Native menu bar. App-level shortcuts are menu accelerators, so macOS shows
//! them in the menu and handles them natively. Menu clicks are forwarded to the
//! frontend as `forma://menu` events whose payload is a command ID.
//! The IDs must match `MENU_COMMANDS` in src/features/commands/commands.ts.

use tauri::menu::{AboutMetadata, MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Runtime};

pub const MENU_EVENT: &str = "forma://menu";

/// (command id, label, accelerator)
const VIEW_ITEMS: [(&str, &str, &str); 2] = [
    ("toggle-sidebar", "Toggle Sidebar", "CmdOrCtrl+Shift+L"),
    ("toggle-ai-panel", "Toggle AI Panel", "CmdOrCtrl+Shift+A"),
];
const GO_ITEMS: [(&str, &str, &str); 2] = [
    ("open-search", "Search…", "CmdOrCtrl+K"),
    ("open-quick-open", "Quick Open…", "CmdOrCtrl+P"),
];
const SETTINGS_ITEM: (&str, &str, &str) = ("open-settings", "Settings…", "CmdOrCtrl+,");
const NEW_PAGE_ITEM: (&str, &str, &str) = ("new-page", "New Page", "CmdOrCtrl+N");

fn is_command(id: &str) -> bool {
    id == SETTINGS_ITEM.0
        || id == NEW_PAGE_ITEM.0
        || VIEW_ITEMS
            .iter()
            .chain(&GO_ITEMS)
            .any(|(cmd, _, _)| *cmd == id)
}

pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let item = |(id, label, accel): (&str, &str, &str)| {
        MenuItemBuilder::with_id(id, label)
            .accelerator(accel)
            .build(app)
    };

    let app_menu = SubmenuBuilder::new(app, "Forma")
        .about(Some(AboutMetadata {
            name: Some("Forma".into()),
            version: Some(app.package_info().version.to_string()),
            comments: Some("A quieter place to think.".into()),
            ..Default::default()
        }))
        .separator()
        .item(&item(SETTINGS_ITEM)?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&item(NEW_PAGE_ITEM)?)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let mut view_menu = SubmenuBuilder::new(app, "View");
    for entry in VIEW_ITEMS {
        view_menu = view_menu.item(&item(entry)?);
    }
    let view_menu = view_menu.separator().fullscreen().build()?;

    let mut go_menu = SubmenuBuilder::new(app, "Go");
    for entry in GO_ITEMS {
        go_menu = go_menu.item(&item(entry)?);
    }
    let go_menu = go_menu.build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &go_menu,
            &window_menu,
        ])
        .build()?;
    app.set_menu(menu)?;
    app.on_menu_event(handle_event);
    Ok(())
}

fn handle_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().as_ref();
    if is_command(id)
        && let Err(err) = app.emit(MENU_EVENT, id)
    {
        log::warn!("couldn't forward menu command {id}: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_known_commands() {
        assert!(is_command("toggle-sidebar"));
        assert!(is_command("open-settings"));
        assert!(is_command("open-quick-open"));
        assert!(!is_command("quit"));
        assert!(!is_command(""));
    }

    #[test]
    fn command_ids_match_frontend() {
        let ts = include_str!("../../src/features/commands/commands.ts");
        for (id, _, _) in VIEW_ITEMS
            .iter()
            .chain(&GO_ITEMS)
            .chain([&SETTINGS_ITEM, &NEW_PAGE_ITEM])
        {
            assert!(
                ts.contains(&format!("\"{id}\"")),
                "{id} missing from commands.ts"
            );
        }
    }
}
