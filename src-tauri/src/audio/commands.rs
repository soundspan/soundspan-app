//! Tauri IPC command handlers for the native audio backend.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::State;

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
        Self { ok: true, error: None }
    }

    #[allow(dead_code)]
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PlayArgs {
    pub url: String,
    pub format: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub autoplay: Option<bool>,
}

#[tauri::command]
pub async fn native_audio_play(
    app: tauri::AppHandle,
    player: State<'_, AudioPlayer>,
    args: PlayArgs,
) -> Result<CommandResult, String> {
    player.play(
        &app,
        args.url,
        args.format,
        args.headers,
        args.autoplay.unwrap_or(true),
    )?;
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_pause(
    player: State<'_, AudioPlayer>,
) -> Result<CommandResult, String> {
    player.pause();
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_resume(
    player: State<'_, AudioPlayer>,
) -> Result<CommandResult, String> {
    player.resume();
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_stop(
    player: State<'_, AudioPlayer>,
) -> Result<CommandResult, String> {
    player.stop();
    Ok(CommandResult::ok())
}

#[derive(Debug, Deserialize)]
pub struct SeekArgs {
    pub position_secs: f64,
}

#[tauri::command]
pub async fn native_audio_seek(
    player: State<'_, AudioPlayer>,
    args: SeekArgs,
) -> Result<CommandResult, String> {
    player.seek(args.position_secs)?;
    Ok(CommandResult::ok())
}

#[derive(Debug, Deserialize)]
pub struct VolumeArgs {
    pub level: f32,
}

#[tauri::command]
pub async fn native_audio_set_volume(
    player: State<'_, AudioPlayer>,
    args: VolumeArgs,
) -> Result<CommandResult, String> {
    player.set_volume(args.level);
    Ok(CommandResult::ok())
}

#[tauri::command]
pub async fn native_audio_get_state(
    player: State<'_, AudioPlayer>,
) -> Result<PlaybackSnapshot, String> {
    Ok(player.get_state())
}

#[derive(Debug, Deserialize)]
pub struct PreloadArgs {
    pub url: String,
    pub format: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

#[tauri::command]
pub async fn native_audio_preload(
    player: State<'_, AudioPlayer>,
    args: PreloadArgs,
) -> Result<CommandResult, String> {
    player.preload(args.url, args.headers)?;
    Ok(CommandResult::ok())
}

#[derive(Debug, Deserialize)]
pub struct ExclusiveArgs {
    pub enabled: bool,
}

#[tauri::command]
pub async fn native_audio_set_exclusive(
    player: State<'_, AudioPlayer>,
    args: ExclusiveArgs,
) -> Result<CommandResult, String> {
    player.set_exclusive(args.enabled);
    Ok(CommandResult::ok())
}
