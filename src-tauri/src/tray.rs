use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show RLCollector", true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start_capture", "Start Capture", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop_capture", "Stop Capture", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &start, &stop, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("RLCollector")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            // start_capture and stop_capture will be handled via frontend events
            _ => {}
        })
        .build(app)?;

    Ok(())
}
