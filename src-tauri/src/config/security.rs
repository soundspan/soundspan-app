use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_store::StoreExt;
use url::Url;

pub fn ensure_local_caller(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let caller_url = window.url().map_err(|e| e.to_string())?;
    if !is_local_app_url(app, &caller_url) {
        return Err(format!(
            "This command is only available from local app content (caller URL: {caller_url})"
        ));
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
    matches!(url.scheme(), "http" | "https") && !is_builtin_local_app_url(url)
}

fn is_local_app_url(app: &AppHandle, url: &Url) -> bool {
    is_builtin_local_app_url(url)
        || app
            .config()
            .build
            .dev_url
            .as_ref()
            .is_some_and(|dev_url| same_origin(url, dev_url))
}

fn is_builtin_local_app_url(url: &Url) -> bool {
    matches!(url.scheme(), "tauri")
        || matches!(url.host_str(), Some("tauri.localhost"))
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::{is_builtin_local_app_url, is_remote, same_origin};
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
        assert!(!is_remote(&Url::parse("http://tauri.localhost").unwrap()));
        assert!(!is_remote(
            &Url::parse("https://tauri.localhost/index.html").unwrap()
        ));
    }

    #[test]
    fn detects_builtin_local_app_urls() {
        assert!(is_builtin_local_app_url(
            &Url::parse("tauri://localhost").unwrap()
        ));
        assert!(is_builtin_local_app_url(
            &Url::parse("http://tauri.localhost/index.html").unwrap()
        ));
        assert!(is_builtin_local_app_url(
            &Url::parse("https://tauri.localhost/index.html").unwrap()
        ));
        assert!(!is_builtin_local_app_url(
            &Url::parse("http://localhost:3030").unwrap()
        ));
    }
}
