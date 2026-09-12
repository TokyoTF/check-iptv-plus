// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// Returns (http_status, elapsed_ms). Caller decides live/dead from the status.
#[tauri::command]
async fn check_url(url: String, timeout_secs: Option<u64>) -> Result<(u16, u64), String> {
    use std::time::{Duration, Instant};

    let timeout = timeout_secs.unwrap_or(8).clamp(2, 60);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;

    let start = Instant::now();
    // Range request: enough to validate the stream without downloading it fully.
    let res = client
        .get(&url)
        .header("Range", "bytes=0-1023")
        .header("User-Agent", "check-iptv-plus/0.1")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    // Pull at most the first chunk so dead 404 pages with 200 + error text
    // still resolve fast; drop the body right after.
    let _ = tokio::time::timeout(Duration::from_secs(timeout), res.bytes())
        .await
        .map_err(|_| "body timeout".to_string())?
        .map_err(|e| e.to_string())?;
    Ok((status, start.elapsed().as_millis() as u64))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![greet, check_url])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
