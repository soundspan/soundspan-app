use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_store::StoreExt;
use url::Url;

pub fn ensure_local_caller(window: &WebviewWindow) -> Result<(), String> {
    let caller_url = window.url().map_err(|e| e.to_string())?;
    if is_remote(&caller_url) {
        return Err("This command is only available from local app content".into());
    }
    Ok(())
}

pub fn ensure_instance_caller(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let caller_url = window.url().map_err(|e| e.to_string())?;
    if !is_remote(&caller_url) {
        return Ok(());
    }

    let store = app.store("config.json").map_err(|e| e.to_string())?;
    let instance_url = store
        .get("instance_url")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| "No configured soundspan instance".to_string())?;
    let instance_url =
        Url::parse(&instance_url).map_err(|e| format!("Invalid stored instance URL: {e}"))?;

    if same_origin(&caller_url, &instance_url) {
        return Ok(());
    }

    Err("Remote IPC is only available to the configured soundspan instance".into())
}

fn is_remote(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::{is_remote, same_origin};
    use url::Url;

    #[test]
    fn matches_same_origin_with_path_differences() {
        let left = Url::parse("https://listen.example.com/player").unwrap();
        let right = Url::parse("https://listen.example.com/api/health").unwrap();
        assert!(same_origin(&left, &right));
    }

    #[test]
    fn rejects_different_ports() {
        let left = Url::parse("https://listen.example.com:8443/").unwrap();
        let right = Url::parse("https://listen.example.com/").unwrap();
        assert!(!same_origin(&left, &right));
    }

    #[test]
    fn detects_remote_urls() {
        assert!(is_remote(
            &Url::parse("https://listen.example.com").unwrap()
        ));
        assert!(!is_remote(&Url::parse("tauri://localhost").unwrap()));
    }
}
