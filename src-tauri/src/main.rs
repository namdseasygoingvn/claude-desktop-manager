#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod admin;
mod cli;
mod commands;
mod core;
mod mcp;
mod platform;
mod tray;
mod tray_icons;
mod updater;
mod window_size;

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
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = tray::show_preferences(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Dispatch(
                        tauri_plugin_log::fern::Dispatch::new()
                            .chain(Box::new(mcp::LogSink) as Box<dyn log::Log>),
                    ),
                ))
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // No args: anything past argv[0] would send the login-item launch down the CLI path.
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::create_profile,
            commands::launch_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::quit_profile,
            commands::list_adoptable,
            commands::adopt_folder,
            commands::open_config,
            commands::reveal_profile,
            commands::doctor,
            commands::is_translated,
            commands::open_releases_page,
            commands::locate_binary,
            commands::open_download_page,
            commands::show_admin_view,
            commands::hide_admin_view,
            commands::toggle_admin_prune,
            commands::get_general_settings,
            commands::set_open_preferences_at_start,
            commands::set_show_usage_limits,
            commands::set_launch_at_login,
            commands::set_theme,
            commands::get_sidebar_width,
            commands::set_sidebar_width,
            commands::get_collapsed_groups,
            commands::set_collapsed_groups,
            commands::list_groups,
            commands::create_group,
            commands::rename_group,
            commands::set_group_icon,
            commands::delete_group,
            commands::set_profile_group,
            commands::move_profile,
            commands::session_sync_status,
            commands::session_sync_join,
            commands::session_sync_leave,
            mcp::commands::get_mcp_status,
            mcp::commands::set_mcp_enabled,
            mcp::commands::set_mcp_port,
            mcp::commands::get_mcp_logs,
            mcp::commands::clear_mcp_logs,
            updater::check_for_updates,
            updater::install_update,
            updater::restart_app,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;
            reconcile_at_startup();
            refresh_login_item(app.handle());
            tray::init(app.handle())?;
            updater::spawn_background_check(app.handle());
            mcp::apply(app.handle());
            let stored = core::settings::load();
            // Before any show(): a frame that repaints after the window is up is a visible flash.
            let _ = tray::apply_theme(app.handle(), stored.theme);
            window_size::restore(app.handle(), stored.window_size);
            // Last, so the tray already exists behind the window the user is about to see.
            // Failing to show is not worth refusing to start over: the tray still works.
            if stored.open_preferences_at_start {
                let _ = tray::show_preferences(app.handle());
            }
            Ok(())
        })
        .on_window_event(tray::on_window_event);

    // Unsupported on Linux/Windows/Android (see the method's own doc comment), so the admin
    // webview's content-process death is only caught here on macOS/iOS; `admin::show`'s load
    // timeout is the fallback everywhere else.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let builder = builder.on_web_content_process_terminate(|webview| {
        if webview.label() == admin::ADMIN_WEBVIEW {
            admin::mark_terminated(tauri::Manager::app_handle(webview));
        } else {
            // Registering this hook at all replaces wry's default silent-reload-on-terminate
            // for every webview, so the main window's own implicit crash recovery must be kept.
            let _ = webview.reload();
        }
    });

    builder
        .run(tauri::generate_context!())
        .expect("failed to start Claude Desktop Manager");
}

/// The login item stores an absolute path to the binary and `is_enabled` only checks that the
/// entry exists, so renaming the binary leaves an entry pointing at nothing while the setting
/// still reads as on. Rewrite it against the running executable.
fn refresh_login_item(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    if manager.is_enabled().unwrap_or(false) {
        if let Err(err) = manager.enable() {
            log::error!("cannot refresh the login item: {err}");
        }
    }
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
