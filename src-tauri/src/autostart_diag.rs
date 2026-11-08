//! 开机自启诊断：当前 exe 与 Run 注册表项对比（Windows）。

use crate::app_log;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle};
use tauri_plugin_autostart::ManagerExt;

const RUN_REGKEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AutostartDiagnostic {
    pub current_exe: String,
    pub registry_value: Option<String>,
    pub registry_exe: Option<String>,
    pub paths_match: bool,
    pub system_enabled: bool,
}

pub fn collect(app: &AppHandle) -> AutostartDiagnostic {
    let app_name = app.package_info().name.clone();
    let current_exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| String::new());
    let registry_value = read_run_value(&app_name);
    let registry_exe = registry_value.as_deref().map(exe_from_run_command);
    let paths_match = match (&current_exe, &registry_exe) {
        (cur, Some(reg)) if !cur.is_empty() => same_exe_path(cur, reg),
        _ => registry_exe.is_none(),
    };
    let system_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    AutostartDiagnostic {
        current_exe,
        registry_value,
        registry_exe,
        paths_match,
        system_enabled,
    }
}

pub fn log_startup(app: &AppHandle) {
    let d = collect(app);
    app_log::info(&format!(
        "autostart diag: system_enabled={} paths_match={} current_exe={} registry_exe={}",
        d.system_enabled,
        d.paths_match,
        d.current_exe,
        d.registry_exe.as_deref().unwrap_or("(none)")
    ));
    if let Some(ref raw) = d.registry_value {
        app_log::info(&format!("autostart diag: registry Run value={raw}"));
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_autostart_diagnostic(app: AppHandle) -> AutostartDiagnostic {
    collect(&app)
}

#[cfg(windows)]
fn read_run_value(app_name: &str) -> Option<String> {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

    read_run_value_from(CURRENT_USER, app_name)
        .or_else(|| read_run_value_from(LOCAL_MACHINE, app_name))
}

#[cfg(windows)]
fn read_run_value_from(root: &windows_registry::Key, app_name: &str) -> Option<String> {
    use windows_result::HRESULT;

    const E_FILENOTFOUND: HRESULT = HRESULT::from_win32(0x80070002_u32);

    match root.open(RUN_REGKEY).and_then(|key| key.get_string(app_name)) {
        Ok(v) => Some(v),
        Err(e) if e.code() == E_FILENOTFOUND => None,
        Err(_) => None,
    }
}

#[cfg(not(windows))]
fn read_run_value(_app_name: &str) -> Option<String> {
    None
}

fn exe_from_run_command(value: &str) -> String {
    let value = value.trim();
    if value.starts_with('"') {
        if let Some(end) = value[1..].find('"') {
            return value[1..1 + end].to_string();
        }
    }
    if let Some(idx) = value.find(" --autostart") {
        return value[..idx].trim().trim_matches('"').to_string();
    }
    value
        .split_whitespace()
        .next()
        .unwrap_or(value)
        .trim_matches('"')
        .to_string()
}

fn same_exe_path(current: &str, registered: &str) -> bool {
    let cur = PathBuf::from(current);
    let reg = PathBuf::from(registered);
    if cur == reg {
        return true;
    }
    match (cur.canonicalize(), reg.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => current.eq_ignore_ascii_case(registered),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unquoted_run_value() {
        assert_eq!(
            exe_from_run_command(r"C:\Program Files\yugutou_tool\yugutou_tool.exe --autostart"),
            r"C:\Program Files\yugutou_tool\yugutou_tool.exe"
        );
    }

    #[test]
    fn parse_quoted_run_value() {
        assert_eq!(
            exe_from_run_command(r#""D:\Apps\yugutou_tool.exe" --autostart"#),
            r"D:\Apps\yugutou_tool.exe"
        );
    }
}
