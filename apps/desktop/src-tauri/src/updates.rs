//! Pre-release update check (#277).
//!
//! The updater plugin's JavaScript API cannot choose an endpoint, and a stable
//! build's configured endpoint (`releases/latest/download/latest.json`) only ever
//! names the newest *stable* release. This command builds the same updater
//! against the pre-release feed and registers the result as a plugin resource
//! exactly as the plugin's own `check` does, so the plugin's download, signature
//! verification and install then apply unchanged. Pre-release builds are signed
//! with the stable updater key, so this adds no new trust.
use std::time::Duration;

use serde::Serialize;
use tauri::{Manager, Runtime, Url, Webview};
use tauri_plugin_updater::UpdaterExt;

/// Published by `pages.yml` from the newest published pre-release's `beta.json`.
const PRERELEASE_FEED: &str = "https://rahulsharmavishwakarma.github.io/vesper/beta.json";

/// A stalled network must surface as "Couldn't check", not an endless check (#321).
/// Matches `CHECK_TIMEOUT_MS` in `updatesTauri.ts`.
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

/// Same field names as the plugin's own update metadata, which the JavaScript
/// `Update` class is constructed from.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrereleaseUpdate {
    rid: u32,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

fn feed_url() -> Url {
    Url::parse(PRERELEASE_FEED).expect("the pre-release feed address is a valid URL")
}

/// Updater authority belongs to the main window only; the quick-capture window
/// receives none (#65).
fn require_main_window(label: &str) -> Result<(), String> {
    if label == "main" {
        Ok(())
    } else {
        Err("Updates are available only from the main window.".into())
    }
}

#[tauri::command]
pub async fn check_prerelease_update<R: Runtime>(
    webview: Webview<R>,
) -> Result<Option<PrereleaseUpdate>, String> {
    require_main_window(webview.label())?;
    let updater = webview
        .updater_builder()
        .endpoints(vec![feed_url()])
        .map_err(|error| error.to_string())?
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    // The manifest carries an RFC 3339 `pub_date`; reuse it as published
    // instead of reformatting the parsed date.
    let date = update
        .raw_json
        .get("pub_date")
        .and_then(|value| value.as_str())
        .map(str::to_owned);
    Ok(Some(PrereleaseUpdate {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date,
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prerelease_feed_is_a_distinct_https_beta_manifest() {
        let url = feed_url();
        assert_eq!(url.scheme(), "https");
        assert!(url
            .host_str()
            .is_some_and(|host| host.ends_with(".github.io")));
        assert!(url.path().ends_with("/beta.json"));
        // It must never alias the stable feed that a stable build is configured with.
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let stable = config["plugins"]["updater"]["endpoints"]
            .as_array()
            .unwrap();
        assert!(!stable.is_empty());
        for endpoint in stable {
            assert_ne!(url.as_str(), endpoint.as_str().unwrap());
        }
    }

    #[test]
    fn only_the_main_window_may_check() {
        assert!(require_main_window("main").is_ok());
        assert!(require_main_window("quick-capture").is_err());
        assert!(require_main_window("").is_err());
    }
}
