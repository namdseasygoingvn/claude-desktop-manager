#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod commands;
mod core;
mod platform;
mod tray;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if let Some(args) = cli_invocation(&args) {
        #[cfg(windows)]
        cli::attach_parent_console();
        std::process::exit(cli::run(args));
    }

    run_manager();
}

/// macOS injects `-psn_0_xxxxx` when the bundle is opened via LaunchServices, which would
/// otherwise look like a subcommand and start the CLI instead of the tray.
fn cli_invocation(args: &[String]) -> Option<Vec<String>> {
    let kept: Vec<String> = args
        .iter()
        .filter(|arg| !arg.starts_with("-psn_"))
        .cloned()
        .collect();

    (kept.len() > 1).then_some(kept)
}

fn run_manager() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = tray::show_manager(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::create_profile,
            commands::launch_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::quit_profile,
            commands::adopt_folder,
            commands::open_config,
            commands::doctor,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;
            reconcile_at_startup();
            tray::init(app.handle())?;
            Ok(())
        })
        .on_window_event(tray::on_window_event)
        .run(tauri::generate_context!())
        .expect("failed to start Claude Desktop Manager");
}

fn reconcile_at_startup() {
    let mut registry = match crate::core::registry::load() {
        Ok(registry) => registry,
        Err(err) => {
            log::error!("registry unavailable: {err}");
            return;
        }
    };

    match crate::core::registry::reconcile(&mut registry) {
        Ok(found) => {
            for discrepancy in &found {
                log::warn!("reconciliation: {discrepancy:?}");
            }
            if let Err(err) = crate::core::registry::save(&registry) {
                log::error!("cannot save the registry: {err}");
            }
        }
        Err(err) => log::error!("reconciliation failed: {err}"),
    }
}
