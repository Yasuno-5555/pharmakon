use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

pub struct TrayHandler {
    _tray_icon: TrayIcon,
}

impl Default for TrayHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl TrayHandler {
    pub fn new() -> Self {
        let menu = Menu::new();
        let show_item = MenuItem::new("Show Dashboard", true, None);
        let reset_item = MenuItem::new("Reset Session", true, None);
        let status_item = MenuItem::new("Status: Online", false, None);
        let quit_item = MenuItem::new("Quit", true, None);

        menu.append_items(&[
            &show_item,
            &reset_item,
            &MenuItem::new("---", false, None),
            &status_item,
            &quit_item,
        ])
        .unwrap();

        let icon = Self::create_icon();

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip("Pharmakon Assistant")
            .with_icon(icon)
            .build()
            .unwrap();

        Self {
            _tray_icon: tray_icon,
        }
    }

    fn create_icon() -> tray_icon::Icon {
        let (width, height) = (32, 32);
        // Purple square as placeholder
        let rgba = vec![128, 0, 128, 255]
            .into_iter()
            .cycle()
            .take(width * height * 4)
            .collect();
        tray_icon::Icon::from_rgba(rgba, width as u32, height as u32).unwrap()
    }

    pub fn handle_events(&self) -> Option<TrayAction> {
        // This should be called in the event loop or a dedicated task
        if let Ok(_event) = MenuEvent::receiver().try_recv() {
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
