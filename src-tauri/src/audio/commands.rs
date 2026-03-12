//! Tauri IPC command handlers for the native audio backend.
#![allow(dead_code)]

use std::collections::HashMap;

use serde::Serialize;
use tauri::State;
use tauri_plugin_store::StoreExt;

use crate::config::security::ensure_instance_caller;

use super::player::AudioPlayer;
use super::state::PlaybackSnapshot;

#[derive(Debug, Serialize)]
pub struct CommandResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CommandResult {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }
}

#[tauri::command]
pub async fn native_audio_play(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
    url: String,
    format: Option<String>,
    headers: Option<HashMap<String, String>>,
    autoplay: Option<bool>,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    player.play(&app, url, format, headers, autoplay.unwrap_or(true))?;
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_pause(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    player.pause(&app);
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_resume(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    player.resume(&app);
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_stop(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    player.stop(&app);
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_seek(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
    position_secs: f64,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    player.seek(&app, position_secs)?;
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_set_volume(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
    level: f32,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    player.set_volume(&app, level);
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_get_state(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
) -> Result<PlaybackSnapshot, String> {
    ensure_instance_caller(&app, &window)?;
    Ok(player.get_state())
}

#[tauri::command]
pub async fn native_audio_preload(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
    url: String,
    format: Option<String>,
    headers: Option<HashMap<String, String>>,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    player.preload(url, format, headers)?;
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_set_exclusive(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    player: State<'_, AudioPlayer>,
    enabled: bool,
) -> Result<CommandResult, String> {
    ensure_instance_caller(&app, &window)?;
    let store = app.store("config.json").map_err(|e| e.to_string())?;
    store.set("wasapi_exclusive_enabled", serde_json::json!(enabled));
    store.save().map_err(|e| e.to_string())?;
    player.set_exclusive(&app, enabled)?;
    Ok(CommandResult::ok())
}
