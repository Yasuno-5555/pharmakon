use tray_icon::{
    menu::{Menu, MenuItem, MenuEvent},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use pharmakon_core::agent::Agent;

pub struct TrayHandler {
    _tray_icon: TrayIcon,
    menu: Menu,
}

impl TrayHandler {
    pub fn new() -> Self {
        let menu = Menu::new();
        let show_item = MenuItem::new("Show Dashboard", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        
        menu.append_items(&[
            &show_item,
            &quit_item,
        ]).unwrap();

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip("Pharmakon Assistant")
            // .with_icon(icon) // TODO: Add icon
            .build()
            .unwrap();

        Self {
            _tray_icon: tray_icon,
            menu,
        }
    }

    pub fn handle_events(&self) -> Option<TrayAction> {
        // This should be called in the event loop or a dedicated task
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            // Match by ID or something
            // For now just check first/second items
            // Actually muda lets us set IDs
            return Some(TrayAction::Show); // Placeholder
        }
        None
    }
}

pub enum TrayAction {
    Show,
    Quit,
}
