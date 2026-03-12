#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;

use tauri::Manager;
use tauri_plugin_store::StoreExt;

use crate::config::security::ensure_local_caller;

// ---------------------------------------------------------------------------
// Instance URL commands
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
struct UrlResult {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[tauri::command]
async fn set_instance_url(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    url: String,
) -> Result<UrlResult, String> {
    ensure_local_caller(&window)?;
    let trimmed = url.trim().trim_end_matches('/').to_string();

    let parsed = match url::Url::parse(&trimmed) {
        Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => parsed,
        Ok(_) => {
            return Ok(UrlResult {
                ok: false,
                error: Some("Instance URL must use http or https".into()),
            });
        }
        Err(_) => {
            return Ok(UrlResult {
                ok: false,
                error: Some("Invalid URL format".into()),
            });
        }
    };

    if parsed.host_str().is_none() {
        return Ok(UrlResult {
            ok: false,
            error: Some("Invalid URL format".into()),
        });
    }

    let health_url = format!("{}/api/health", trimmed);
    let client = reqwest::Client::new();
    match client
        .get(&health_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => {
            return Ok(UrlResult {
                ok: false,
                error: Some(format!("Health check returned status {}", resp.status())),
            });
        }
        Err(e) => {
            return Ok(UrlResult {
                ok: false,
                error: Some(format!("Could not reach instance: {}", e)),
            });
        }
    }

    let store = app.store("config.json").map_err(|e| e.to_string())?;
    store.set("instance_url", serde_json::json!(&trimmed));
    store.save().map_err(|e| e.to_string())?;

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.navigate(parsed);
    }

    Ok(UrlResult {
        ok: true,
        error: None,
    })
}

#[tauri::command]
async fn get_instance_url(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<String>, String> {
    ensure_local_caller(&window)?;
    let store = app.store("config.json").map_err(|e| e.to_string())?;
    let url = store
        .get("instance_url")
        .and_then(|v| v.as_str().map(String::from));
    Ok(url)
}

// ---------------------------------------------------------------------------
// App entry
// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .manage(audio::player::AudioPlayer::new())
        .invoke_handler(tauri::generate_handler![
            set_instance_url,
            get_instance_url,
            audio::commands::native_audio_play,
            audio::commands::native_audio_pause,
            audio::commands::native_audio_resume,
            audio::commands::native_audio_stop,
            audio::commands::native_audio_seek,
            audio::commands::native_audio_set_volume,
            audio::commands::native_audio_get_state,
            audio::commands::native_audio_preload,
            audio::commands::native_audio_set_exclusive,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let store = match handle.store("config.json") {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if let Some(enabled) = store
                    .get("wasapi_exclusive_enabled")
                    .and_then(|v| v.as_bool())
                {
                    let player = handle.state::<audio::player::AudioPlayer>();
                    player.restore_exclusive(enabled);
                }
                if let Some(url_str) = store
                    .get("instance_url")
                    .and_then(|v| v.as_str().map(String::from))
                {
                    if let Some(window) = handle.get_webview_window("main") {
                        if let Ok(parsed) = url_str.parse::<url::Url>() {
                            let _ = window.navigate(parsed);
                        }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running soundspan");
}
