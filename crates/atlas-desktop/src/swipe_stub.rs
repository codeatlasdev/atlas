//! Non-macOS: no native swipe tracking; the frontend keeps its wheel recognizer.

#[tauri::command]
pub fn swipe_zones(_zones: Vec<serde_json::Value>, _scrollers: Option<Vec<serde_json::Value>>) {}

#[tauri::command]
pub fn swipe_zones_now() -> serde_json::Value {
    serde_json::Value::Array(Vec::new())
}

#[tauri::command]
pub async fn swipe_native() -> bool {
    false
}

#[tauri::command]
pub async fn swipe_go(_zone: String, _id: Option<String>, _dir: i8) -> bool {
    false
}
