//! GTK state events describe what the compositor granted, including transitions
//! that do not resize the window. GTK3 does not expose xdg wm_capabilities.
use serde::Serialize;
use tauri::WebviewWindow;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowControlState {
    can_minimize: bool,
    can_maximize: bool,
    maximized: bool,
    fullscreen: bool,
    minimized: bool,
}

#[cfg(any(target_os = "linux", test))]
fn can_minimize(
    wayland: bool,
    hyprland_signature: Option<&str>,
    display_name: &str,
    lock: Option<&str>,
) -> bool {
    let positive = hyprland_signature.is_some_and(|s| !s.trim().is_empty())
        && lock
            .and_then(|text| text.lines().nth(1))
            .is_some_and(|socket| socket.trim() == display_name);
    !(wayland && positive)
}

#[cfg(target_os = "linux")]
fn gtk_state(window: &gtk::ApplicationWindow) -> WindowControlState {
    use gtk::prelude::*;
    let gdk_window = window.window();
    let wayland = window.display().type_().name() == "GdkWaylandDisplay";
    let state = gdk_window
        .map(|w| w.state())
        .unwrap_or_else(gtk::gdk::WindowState::empty);
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
    let lock = signature
        .as_ref()
        .filter(|s| !s.contains('/') && !s.contains('\\'))
        .and_then(|s| {
            let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
            std::fs::read_to_string(
                std::path::PathBuf::from(runtime)
                    .join("hypr")
                    .join(s)
                    .join("hyprland.lock"),
            )
            .ok()
        });
    WindowControlState {
        can_minimize: can_minimize(
            wayland,
            signature.as_deref(),
            &window.display().name(),
            lock.as_deref(),
        ),
        can_maximize: window.is_resizable(),
        maximized: state.contains(gtk::gdk::WindowState::MAXIMIZED),
        fullscreen: state.contains(gtk::gdk::WindowState::FULLSCREEN),
        minimized: state.contains(gtk::gdk::WindowState::ICONIFIED),
    }
}

#[tauri::command]
pub async fn get_window_control_state(window: WebviewWindow) -> Result<WindowControlState, String> {
    #[cfg(target_os = "linux")]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        let target = window.clone();
        window
            .run_on_main_thread(move || {
                let result = target
                    .gtk_window()
                    .map(|gtk| gtk_state(&gtk))
                    .map_err(|e| e.to_string());
                let _ = send.send(result);
            })
            .map_err(|e| e.to_string())?;
        receive.await.map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(WindowControlState {
            can_minimize: window.is_minimizable().unwrap_or(true),
            can_maximize: window.is_maximizable().unwrap_or(true),
            maximized: window.is_maximized().map_err(|e| e.to_string())?,
            fullscreen: window.is_fullscreen().map_err(|e| e.to_string())?,
            minimized: window.is_minimized().unwrap_or(false),
        })
    }
}

pub fn install(window: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        use tauri::Emitter;
        let target = window.clone();
        let _ = window.run_on_main_thread(move || {
            if let Ok(gtk) = target.gtk_window() {
                let state_target = target.clone();
                gtk.connect_window_state_event(move |window, _event| {
                    let _ = state_target.emit_to(
                        tauri::EventTarget::WebviewWindow {
                            label: state_target.label().to_owned(),
                        },
                        "window-control-state",
                        gtk_state(window),
                    );
                    gtk::glib::Propagation::Proceed
                });
                gtk.connect_resizable_notify(move |window| {
                    let _ = target.emit_to(
                        tauri::EventTarget::WebviewWindow {
                            label: target.label().to_owned(),
                        },
                        "window-control-state",
                        gtk_state(window),
                    );
                });
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = window;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_native_wayland_with_a_positive_hyprland_signature_hides_minimize() {
        assert!(!can_minimize(
            true,
            Some("hyprland-session-id"),
            "wayland-1",
            Some("123\nwayland-1\n")
        ));
        assert!(can_minimize(
            false,
            Some("hyprland-session-id"),
            "wayland-1",
            Some("123\nwayland-1\n")
        ));
        assert!(can_minimize(true, None, "wayland-1", None));
        assert!(can_minimize(
            true,
            Some("hyprland-session-id"),
            "private-weston",
            Some("123\nwayland-1\n")
        ));
        assert!(can_minimize(true, Some(""), "wayland-1", None));
        assert!(can_minimize(true, Some("  "), "wayland-1", None));
    }
}

#[cfg(test)]
mod settings_tests {
    #[test]
    fn window_control_override_survives_settings_and_old_files_default_to_auto() {
        let mut settings = crate::app_settings::AppSettings::default();
        let mut old = serde_json::to_value(&settings).unwrap();
        old.as_object_mut().unwrap().remove("windowControls");
        let loaded: crate::app_settings::AppSettings = serde_json::from_value(old).unwrap();
        assert!(loaded.window_controls.is_none());
        settings.window_controls = Some("always".into());
        let restored: crate::app_settings::AppSettings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.window_controls.as_deref(), Some("always"));
    }
}
