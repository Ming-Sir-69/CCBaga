// [INPUT]: Read-only platform registrations and whitelisted browser metadata.
// [OUTPUT]: Installed/default/history/profile discovery; import capability stays unavailable.
// [POS]: No environment mutations, client-supplied paths, secret fields or migration commands.
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Installation {
    pub bundle_id: String,
    pub name: String,
    pub path: String,
    pub version: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct DiscoveredBrowser {
    pub id: String,
    pub name: String,
    pub bundle_id: String,
    pub kind: String,
    pub installed: bool,
    pub is_default: bool,
    pub installations: Vec<Installation>,
    pub data_root: Option<String>,
    pub data_state: String,
    pub profile_state: String,
    pub profiles: Vec<Profile>,
    pub import_capability: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Discovery {
    pub supported: bool,
    pub default_known: bool,
    pub items: Vec<DiscoveredBrowser>,
}
#[cfg(target_os = "macos")]
extern "C" { fn rf_browser_discover(profiles_enabled: bool) -> *mut std::ffi::c_char; }

#[tauri::command]
pub async fn browser_discover(profiles_enabled: bool) -> Result<Discovery, String> {
    super::run(move || {
        #[cfg(target_os = "macos")]
        { crate::native::decode(unsafe { rf_browser_discover(profiles_enabled) }) }
        #[cfg(not(target_os = "macos"))]
        { let _ = profiles_enabled; Ok(Discovery { supported: false, default_known: false, items: vec![] }) }
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_contract_keeps_absent_software_distinct_from_historical_data() {
        let payload = serde_json::json!({"supported":true,"default_known":false,"items":[{
            "id":"com.google.Chrome","name":"Chrome","bundle_id":"com.google.Chrome","kind":"chromium",
            "installed":false,"is_default":false,"installations":[],"data_root":"/fixture/Chrome",
            "data_state":"present","profile_state":"ready","profiles":[{"id":"Default","name":"Synthetic"}],
            "import_capability":"unavailable"
        }]});
        let report: Discovery = serde_json::from_value(payload).unwrap();
        assert!(!report.items[0].installed);
        assert_eq!(report.items[0].data_state, "present");
        assert_eq!(report.items[0].import_capability, "unavailable");
        let roundtrip = serde_json::to_value(report).unwrap();
        assert_eq!(roundtrip["items"][0]["profiles"][0]["name"], "Synthetic");
    }
}
