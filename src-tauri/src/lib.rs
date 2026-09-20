mod jobs;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load development secrets for the Rust backend. Existing environment
    // variables take precedence over values from `.env`.
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(jobs::manager_for(&app.handle())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            jobs::start_channel_clipping_job,
            jobs::start_vod_clipping_job,
            jobs::cancel_clipping_job,
            jobs::retry_clipping_job,
            jobs::get_clipping_job,
            jobs::list_clipping_jobs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
