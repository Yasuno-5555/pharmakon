pub mod app;
pub mod widgets;
pub mod tray;

pub use app::{AppData, ViewType};
use std::sync::Arc;
use tokio::sync::Mutex;
use pharmakon_core::agent::Agent;
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::automation::cron::CronManager;
use tray::TrayHandler;

use xilem::{EventLoop, Xilem, AppState};
use masonry_winit::app::MasonryUserEvent;
use tray_icon::menu::MenuEvent;
use std::sync::atomic::Ordering;

fn app_logic_wrapper(data: &mut AppData) -> std::vec::IntoIter<xilem::WindowView<AppData>> {
    app::app_logic(data).into_iter()
}

impl xilem::AppState for AppData {
    fn keep_running(&self) -> bool {
        true
    }
}

pub fn run_app(
    agent: Arc<Mutex<Agent>>,
    db: Arc<DbSessionStore>,
    cron_manager: Arc<CronManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app_data = AppData::new(agent, db, cron_manager);
    
    // Create Event Loop first to get proxy
    let event_loop = EventLoop::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    
    // Initialize Tray
    let _tray = TrayHandler::new();
    
    // Spawn tray event handler
    let proxy_clone = proxy.clone();
    let show_requested = app_data.show_requested.clone();
    tokio::spawn(async move {
        let receiver = MenuEvent::receiver();
        while let Ok(_event) = receiver.recv() {
            show_requested.store(true, Ordering::SeqCst);
            // Sending a dummy event to wake up the event loop
            let _ = proxy_clone.send_event(MasonryUserEvent::Action(
                xilem::WindowId::next(),
                Box::new(()),
                xilem::masonry::core::WidgetId::reserved(0),
            ));
        }
    });
    
    let app = Xilem::new(
        app_data, 
        app_logic_wrapper, 
    );
    
    let (driver, windows) = app.into_driver_and_windows(move |event| {
        proxy.send_event(event).map_err(|err| err.0)
    });
    
    masonry_winit::app::run_with(
        event_loop, 
        windows, 
        driver, 
        xilem::masonry::theme::default_property_set()
    )?;
    
    Ok(())
}
