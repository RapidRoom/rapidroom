use serde_json::{Value, json};
use tauri::AppHandle;
#[cfg(feature = "mcp")]
use tauri::Manager;

#[cfg(feature = "mcp")]
pub(crate) fn persist_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let path = crate::app_settings::get_settings_path(app)?;
    let mut settings = if path.exists() {
        serde_json::from_str::<Value>(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        json!({})
    };
    settings
        .as_object_mut()
        .ok_or("Settings must be a JSON object")?
        .insert("mcpEnabled".into(), json!(enabled));
    crate::file_management::write_file_atomically(
        &path,
        serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mcp_control_status(app_handle: AppHandle) -> Value {
    #[cfg(feature = "mcp")]
    let port = *app_handle
        .state::<crate::AppState>()
        .mcp
        .port
        .lock()
        .unwrap();
    #[cfg(not(feature = "mcp"))]
    let port = {
        let _ = app_handle;
        0_u16
    };
    json!({"available":cfg!(feature="mcp"),"enabled":port!=0,"port":port})
}

#[tauri::command]
pub async fn set_mcp_enabled(enabled: bool, app_handle: AppHandle) -> Result<Value, String> {
    #[cfg(feature = "mcp")]
    crate::mcp::set_enabled(app_handle.clone(), enabled, true).await?;
    #[cfg(not(feature = "mcp"))]
    if enabled {
        return Err("This build does not include AI control".into());
    }
    Ok(mcp_control_status(app_handle))
}
