//! 系统托盘：电量图标、设备菜单。电量采样在 `battery`，这里只负责画出来。

use crate::app_log;
use crate::battery;
use crate::devices;
use crate::plugin_sdk::{BatteryInfo, DeviceCategory, DeviceMeta};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Same source as frontend settings.autoIntervalSec (DEFAULT = 300).
const DEFAULT_AUTO_INTERVAL_SEC: u64 = 300;

/// Read auto refresh interval (seconds) from zustand settings store.
pub(crate) fn auto_interval_sec(app: &AppHandle) -> u64 {
    use tauri_plugin_zustand::ManagerExt;
    let raw = app
        .zustand()
        .get_or::<u64>("settings", "autoIntervalSec", DEFAULT_AUTO_INTERVAL_SEC);
    raw.clamp(3, 300)
}

/// 首页「暂停自动刷新」写在同一份 settings store 里。缺省开启。
pub(crate) fn auto_refresh_enabled(app: &AppHandle) -> bool {
    use tauri_plugin_zustand::ManagerExt;
    app.zustand()
        .get_or::<bool>("settings", "autoRefresh", true)
}

/// 托盘菜单与电量都走设备插件表，不在这里维护第二份 id / 名称。
fn resolve_stored_device_id(stored: String) -> String {
    if devices::find(&stored).is_some() {
        stored
    } else {
        devices::default_device_id()
    }
}

pub(crate) fn read_stored_tray_device_id(app: &AppHandle) -> String {
    use tauri_plugin_zustand::ManagerExt;
    let stored =
        app.zustand()
            .get_or::<String>("settings", "trayDeviceId", devices::default_device_id());
    resolve_stored_device_id(stored)
}

/// 从托盘恢复并聚焦主窗口。
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// 读取前端持久化的关闭行为：`quit` | `tray`（默认托盘）。
pub fn close_behavior(app: &AppHandle) -> String {
    use tauri_plugin_zustand::ManagerExt;
    app.zustand()
        .get_or::<String>("settings", "closeBehavior", "tray".into())
}

/// 一次写入托盘设备和分类选中项，避免两次 patch 触发两次电量读取。
fn commit_tray_device(app: &AppHandle, device_id: String, selected: SelectedByCategory) {
    use tauri_plugin_zustand::ManagerExt;
    let id = resolve_stored_device_id(device_id);
    let mut selected = sanitize_selection(&selected);
    if let Some(plugin) = devices::find(&id) {
        selected.set(plugin.meta().category, id.clone());
    }
    let selection = serde_json::to_value(&selected).unwrap_or(Value::Null);
    let _ = app.zustand().patch(
        "settings",
        [
            ("trayDeviceId", Value::String(id)),
            ("selectedByCategory", selection),
        ],
    );
}

/// 每个分类至多一只。与前端 `SelectedByCategory` 同一份字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct SelectedByCategory {
    #[serde(default)]
    mouse: Option<String>,
    #[serde(default)]
    keyboard: Option<String>,
    #[serde(default)]
    headset: Option<String>,
}

impl SelectedByCategory {
    fn get(&self, category: DeviceCategory) -> Option<&str> {
        match category {
            DeviceCategory::Mouse => self.mouse.as_deref(),
            DeviceCategory::Keyboard => self.keyboard.as_deref(),
            DeviceCategory::Headset => self.headset.as_deref(),
        }
    }

    fn set(&mut self, category: DeviceCategory, id: String) {
        let slot = match category {
            DeviceCategory::Mouse => &mut self.mouse,
            DeviceCategory::Keyboard => &mut self.keyboard,
            DeviceCategory::Headset => &mut self.headset,
        };
        *slot = Some(id);
    }
}

fn read_selected_by_category(app: &AppHandle) -> SelectedByCategory {
    use tauri_plugin_zustand::ManagerExt;
    app.zustand().get_or(
        "settings",
        "selectedByCategory",
        SelectedByCategory::default(),
    )
}

/// 主窗口选中设备时调用：记下该分类的选中项，并让托盘图标跟着这只。
/// 电量由 settings 监听触发的同一次采样更新，这里不再单独读 HID。
#[tauri::command]
#[specta::specta]
pub fn set_tray_device(
    app: AppHandle,
    device_id: String,
    selected_by_category: SelectedByCategory,
) {
    commit_tray_device(&app, device_id, selected_by_category);
}

fn put_px(buf: &mut [u8], size: u32, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
        return;
    }
    let i = ((y as u32 * size + x as u32) * 4) as usize;
    buf[i] = r;
    buf[i + 1] = g;
    buf[i + 2] = b;
    buf[i + 3] = a;
}

/// 托盘图标上一格电量：外层 `None` = 该分类未选设备；内层 `None` = 已选但尚无百分比。
type TraySlotLevel = Option<Option<u8>>;

/// 与旧单电池图标相同的档位色：≤15 红、≤30 橙、其余绿；未知灰。
fn battery_fill_rgb(percent: Option<u8>) -> (u8, u8, u8) {
    match percent {
        Some(p) if p <= 15 => (239, 68, 68),
        Some(p) if p <= 30 => (245, 158, 11),
        Some(_) => (52, 211, 153),
        None => (148, 163, 184),
    }
}

fn default_tray_category_keys() -> Vec<String> {
    vec![
        "mouse".into(),
        "keyboard".into(),
        "headset".into(),
    ]
}

/// 读取设置里的托盘分类多选，规范化为 鼠标→键盘→耳机；至少一项。
fn read_tray_categories(app: &AppHandle) -> Vec<DeviceCategory> {
    use tauri_plugin_zustand::ManagerExt;
    let raw = app.zustand().get_or::<Vec<String>>(
        "settings",
        "trayCategories",
        default_tray_category_keys(),
    );
    sanitize_tray_categories(&raw)
}

fn sanitize_tray_categories(raw: &[String]) -> Vec<DeviceCategory> {
    const ORDER: [DeviceCategory; 3] = [
        DeviceCategory::Mouse,
        DeviceCategory::Keyboard,
        DeviceCategory::Headset,
    ];
    let mut out = Vec::new();
    for category in ORDER {
        let key = category_key(category);
        if raw.iter().any(|s| s == key) {
            out.push(category);
        }
    }
    if out.is_empty() {
        out.push(DeviceCategory::Mouse);
    }
    out
}

/// 按「托盘显示分类」过滤后的电量格（1～3）。未启用的分类不出现。
fn tray_visible_levels(
    active_id: &str,
    selected: &SelectedByCategory,
    categories: &[DeviceCategory],
    samples: &HashMap<String, BatteryInfo>,
) -> Vec<TraySlotLevel> {
    let resolved = resolve_selection(active_id, selected);
    let mut levels = Vec::new();
    for &category in categories {
        let level = match resolved
            .get(category)
            .and_then(|id| matching_meta(id, category))
        {
            Some(meta) => Some(samples.get(&meta.id).and_then(|info| info.percent)),
            None => None,
        };
        levels.push(level);
    }
    if levels.is_empty() {
        levels.push(None);
    }
    levels
}

/// 在 32×32 上画电池：只选 1 类 → 单颗横向大电池；≥2 类 → 并排竖向小电池。
fn make_tray_battery_icon(slots: &[TraySlotLevel]) -> Image<'static> {
    const S: u32 = 32;
    // 未绘制像素保持 RGBA(0,0,0,0)，Windows 托盘才能透出任务栏，避免「大黑边」。
    let mut buf = vec![0u8; (S * S * 4) as usize];

    let n = slots.len().clamp(1, 3);
    if n == 1 {
        // 只选一类：单颗横向大电池（接近旧单电池样式）
        draw_horizontal_battery(&mut buf, S, 2, 8, 28, 16, slots[0]);
    } else {
        // ≥2：并排竖向小电池
        let origins = vertical_battery_origins(n);
        for (slot, &ox) in slots.iter().take(n).zip(origins.iter()) {
            draw_vertical_battery(&mut buf, S, ox, *slot);
        }
    }

    Image::new_owned(buf, S, S)
}

/// 并排竖条的左缘 x：2 颗宽约 14；3 颗宽约 10。
fn vertical_battery_origins(n: usize) -> Vec<i32> {
    match n.clamp(2, 3) {
        2 => vec![2, 17],
        _ => vec![0, 11, 22],
    }
}

/// 竖向迷你电池：正极在上，双线描边，电量自底向上。
fn draw_vertical_battery(buf: &mut [u8], size: u32, ox: i32, slot: TraySlotLevel) {
    // 2 列时更宽（14），3 列时宽 10
    let w = if ox == 2 || ox == 17 { 13 } else { 10 };
    let selected = slot.is_some();
    let (frame_r, frame_g, frame_b) = if selected {
        (210u8, 210, 220)
    } else {
        (78, 78, 90)
    };

    // 正极帽
    let tip_x0 = ox + w / 4;
    let tip_x1 = ox + w - w / 4;
    for y in 1..4 {
        for x in tip_x0..tip_x1 {
            put_px(buf, size, x, y, frame_r, frame_g, frame_b, 255);
        }
    }
    // 外框双线顶/底
    for y in 4..6 {
        for x in ox..(ox + w) {
            put_px(buf, size, x, y, frame_r, frame_g, frame_b, 255);
        }
    }
    for y in 29..31 {
        for x in ox..(ox + w) {
            put_px(buf, size, x, y, frame_r, frame_g, frame_b, 255);
        }
    }
    // 外框双线左右
    for y in 4..31 {
        put_px(buf, size, ox, y, frame_r, frame_g, frame_b, 255);
        put_px(buf, size, ox + 1, y, frame_r, frame_g, frame_b, 255);
        put_px(buf, size, ox + w - 2, y, frame_r, frame_g, frame_b, 255);
        put_px(buf, size, ox + w - 1, y, frame_r, frame_g, frame_b, 255);
    }

    let inner_x0 = ox + 2;
    let inner_x1 = ox + w - 2;
    match slot {
        None => {}
        Some(None) => {
            for y in 14..18 {
                for x in inner_x0..inner_x1 {
                    put_px(buf, size, x, y, 110, 110, 120, 255);
                }
            }
        }
        Some(Some(p)) => {
            let (fr, fg, fb) = battery_fill_rgb(Some(p));
            let inner_h = 23;
            let filled = (inner_h * p as i32 / 100).clamp(0, inner_h);
            for dy in 0..filled {
                let y = 28 - dy;
                for x in inner_x0..inner_x1 {
                    put_px(buf, size, x, y, fr, fg, fb, 255);
                }
            }
        }
    }
}

/// 横向大电池：本体在左，正极帽在右；双线描边；电量自左向右（旧单电池风格）。
fn draw_horizontal_battery(
    buf: &mut [u8],
    size: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    slot: TraySlotLevel,
) {
    let selected = slot.is_some();
    let (frame_r, frame_g, frame_b) = if selected {
        (210u8, 210, 220)
    } else {
        (78, 78, 90)
    };

    let tip_w = 3i32;
    let body_w = (w - tip_w).max(8);
    let thick = 2i32;

    // 正极帽（右侧）
    let tip_x0 = x + body_w;
    let tip_y0 = y + h / 4;
    let tip_y1 = y + h - h / 4;
    for py in tip_y0..tip_y1 {
        for px in tip_x0..(x + w) {
            put_px(buf, size, px, py, frame_r, frame_g, frame_b, 255);
        }
    }

    for t in 0..thick {
        for px in x..(x + body_w) {
            put_px(buf, size, px, y + t, frame_r, frame_g, frame_b, 255);
            put_px(buf, size, px, y + h - 1 - t, frame_r, frame_g, frame_b, 255);
        }
    }
    for t in 0..thick {
        for py in y..(y + h) {
            put_px(buf, size, x + t, py, frame_r, frame_g, frame_b, 255);
            put_px(buf, size, x + body_w - 1 - t, py, frame_r, frame_g, frame_b, 255);
        }
    }

    let inner_x0 = x + thick;
    let inner_x1 = x + body_w - thick;
    let inner_y0 = y + thick;
    let inner_y1 = y + h - thick;
    let inner_w = (inner_x1 - inner_x0).max(0);

    match slot {
        None => {}
        Some(None) => {
            let cx = inner_x0 + inner_w / 2;
            for py in inner_y0..inner_y1 {
                for px in (cx - 1)..(cx + 2) {
                    put_px(buf, size, px, py, 110, 110, 120, 255);
                }
            }
        }
        Some(Some(p)) => {
            let (fr, fg, fb) = battery_fill_rgb(Some(p));
            let filled = (inner_w * p as i32 / 100).clamp(0, inner_w);
            for dx in 0..filled {
                let px = inner_x0 + dx;
                for py in inner_y0..inner_y1 {
                    put_px(buf, size, px, py, fr, fg, fb, 255);
                }
            }
        }
    }
}

fn category_label(category: DeviceCategory) -> &'static str {
    match category {
        DeviceCategory::Mouse => "鼠标",
        DeviceCategory::Keyboard => "键盘",
        DeviceCategory::Headset => "耳机",
    }
}

fn matching_meta(id: &str, category: DeviceCategory) -> Option<DeviceMeta> {
    let meta = devices::find(id)?.meta();
    (meta.category == category).then_some(meta)
}

/// 丢掉对不上的 id。当前电量设备所在分类还空着时，用它补上。
/// 和前端 `resolveSelectedByCategory` 同一条规则。
fn resolve_selection(active_id: &str, selected: &SelectedByCategory) -> SelectedByCategory {
    let mut clean = sanitize_selection(selected);
    if let Some(plugin) = devices::find(active_id) {
        let meta = plugin.meta();
        if clean.get(meta.category).is_none() {
            clean.set(meta.category, meta.id);
        }
    }
    clean
}

fn sanitize_selection(selected: &SelectedByCategory) -> SelectedByCategory {
    const ORDER: [DeviceCategory; 3] = [
        DeviceCategory::Mouse,
        DeviceCategory::Keyboard,
        DeviceCategory::Headset,
    ];
    let mut clean = SelectedByCategory::default();
    for category in ORDER {
        if let Some(id) = selected.get(category) {
            if let Some(meta) = matching_meta(id, category) {
                clean.set(category, meta.id);
            }
        }
    }
    clean
}

fn category_key(category: DeviceCategory) -> &'static str {
    match category {
        DeviceCategory::Mouse => "mouse",
        DeviceCategory::Keyboard => "keyboard",
        DeviceCategory::Headset => "headset",
    }
}

fn battery_status(info: Option<&BatteryInfo>) -> String {
    match info {
        // 型号不受支持：直说，不能和「读不到电量」混在一起。
        Some(info) if info.unsupported => "型号不支持".into(),
        Some(info) => match info.percent {
            Some(percent) => format!("{percent}%"),
            // 充电但没有百分比（如 S9 Pro 的 0xE4 哨兵值）：说「充电中」，
            // 不能说「电量未知」——那不是读失败，是厂商此时不给百分比。
            None if info.charging == Some(true) => "充电中".into(),
            None if info.connected => "电量未知".into(),
            None => "未连接".into(),
        },
        None => "读取中…".into(),
    }
}

/// 托盘图标要画成哪种形态。
///
/// 「充电无百分比」和「型号不支持」都**不能画成 0% 电量**——前者是厂商不给数，
/// 后者是这只设备根本不归本插件管，画 0% 都会让用户以为没电了。
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum IconMode {
    /// 有百分比（或完全没采样到）。
    Level(Option<u8>),
    /// 充电中但拿不到百分比：画闪电。
    Charging,
    /// 型号不受支持：画感叹号。
    Unsupported,
}

#[allow(dead_code)]
fn icon_mode(info: Option<&BatteryInfo>) -> IconMode {
    match info {
        Some(info) if info.unsupported => IconMode::Unsupported,
        Some(info) if info.percent.is_none() && info.charging == Some(true) => IconMode::Charging,
        Some(info) => IconMode::Level(info.percent),
        None => IconMode::Level(None),
    }
}

/// 悬停提示只列出「当前选择」的鼠标 / 键盘 / 耳机（最多三行）。
/// 未选的分类不出现；打勾的是托盘图标数字跟着的那一只。
/// 悬停提示只列出设置里勾选的分类（鼠标→键盘→耳机）；未勾选的不出现。
pub(crate) fn tooltip_text(
    active_id: &str,
    selected: &SelectedByCategory,
    categories: &[DeviceCategory],
    samples: &HashMap<String, BatteryInfo>,
) -> String {
    let resolved = resolve_selection(active_id, selected);
    let mut lines = Vec::new();
    for &category in categories {
        match resolved
            .get(category)
            .and_then(|id| matching_meta(id, category))
        {
            Some(meta) => {
                let status = battery_status(samples.get(&meta.id));
                if meta.id == active_id {
                    lines.push(format!("✓ {} · {status}", meta.name));
                } else {
                    lines.push(format!("{} · {status}", meta.name));
                }
            }
            None => {
                lines.push(format!("{} · 未选择", category_label(category)));
            }
        }
    }
    if lines.is_empty() {
        "yugutou_tool".into()
    } else {
        lines.join("
")
    }
}

/// 右键菜单只保留打开主界面与退出。
fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let show = MenuItem::with_id(app, "show", "打开主界面", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    Menu::with_items(app, &[&show, &quit])
}

/// 每只设备最近一次电量采样；图标与 tip 按托盘分类设置绘制。
struct TrayBoard {
    samples: Mutex<HashMap<String, BatteryInfo>>,
}

fn sample_snapshot(app: &AppHandle) -> HashMap<String, BatteryInfo> {
    app.try_state::<TrayBoard>()
        .map(|board| {
            board
                .samples
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .clone()
        })
        .unwrap_or_default()
}

fn redraw_tray(app: &AppHandle) {
    let samples = sample_snapshot(app);
    let active = read_stored_tray_device_id(app);
    let selected = read_selected_by_category(app);
    let categories = read_tray_categories(app);
    let tip = tooltip_text(&active, &selected, &categories, &samples);
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    let _ = tray.set_tooltip(Some(&tip));
    let levels = tray_visible_levels(&active, &selected, &categories, &samples);
    let _ = tray.set_icon(Some(make_tray_battery_icon(&levels)));
    if let Ok(menu) = build_tray_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub(crate) fn publish_battery(app: &AppHandle, device_id: &str, info: &BatteryInfo) {
    if !info.connected {
        app_log::warn(&format!(
            "tray battery device={device_id} disconnected detail={}",
            info.detail
        ));
    }
    if let Some(board) = app.try_state::<TrayBoard>() {
        board
            .samples
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(device_id.to_string(), info.clone());
    }
    redraw_tray(app);
}

pub(crate) fn refresh_tray_menu(app: &AppHandle) {
    redraw_tray(app);
}

#[derive(Clone)]
struct SettingsSnap {
    tray_id: String,
    auto: bool,
    interval: u64,
    selected: SelectedByCategory,
    tray_categories: Vec<DeviceCategory>,
}

impl Default for SettingsSnap {
    fn default() -> Self {
        Self {
            tray_id: String::new(),
            auto: true,
            interval: DEFAULT_AUTO_INTERVAL_SEC,
            selected: SelectedByCategory::default(),
            tray_categories: vec![
                DeviceCategory::Mouse,
                DeviceCategory::Keyboard,
                DeviceCategory::Headset,
            ],
        }
    }
}

struct SettingsWatch {
    snap: Mutex<SettingsSnap>,
}

fn read_settings_snap(app: &AppHandle) -> SettingsSnap {
    SettingsSnap {
        tray_id: read_stored_tray_device_id(app),
        auto: auto_refresh_enabled(app),
        interval: auto_interval_sec(app),
        selected: read_selected_by_category(app),
        tray_categories: read_tray_categories(app),
    }
}

/// 对照上一份快照决定要不要读电量。
/// 换设备才作废进行中的读取并立刻再采一次。只改分类选中项时重画菜单。
/// 改间隔或暂停只打断睡眠；从暂停恢复才立刻读。
fn on_settings_changed(app: &AppHandle) {
    let Some(state) = app.try_state::<SettingsWatch>() else {
        return;
    };
    // 在同一把锁里读 store 再换快照。两个回调不会拿着同一份旧值，各触发一次采样。
    let (prev, next) = {
        let mut snap = state.snap.lock().unwrap_or_else(|err| err.into_inner());
        let prev = snap.clone();
        let next = read_settings_snap(app);
        *snap = next.clone();
        (prev, next)
    };
    if next.tray_id != prev.tray_id {
        battery::invalidate_device(app);
        battery::request_refresh(app);
        refresh_tray_menu(app);
    } else if next.selected != prev.selected
        || next.tray_categories != prev.tray_categories
    {
        refresh_tray_menu(app);
    }
    if next.interval != prev.interval {
        battery::poke(app);
    }
    if next.auto != prev.auto {
        if next.auto {
            battery::request_refresh(app);
        } else {
            battery::poke(app);
        }
    }
}

/// 等前端把 settings store 拉起来，再开始监听。这样启动采样用的是磁盘上的设备。
pub(crate) fn install_settings_watch(app: &AppHandle) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        use tauri_plugin_zustand::ManagerExt;
        if app
            .zustand()
            .get::<String>("settings", "trayDeviceId")
            .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    if let Some(state) = app.try_state::<SettingsWatch>() {
        let mut snap = state.snap.lock().unwrap_or_else(|err| err.into_inner());
        *snap = read_settings_snap(app);
    }
    for _ in 0..10 {
        use tauri_plugin_zustand::ManagerExt;
        match app.zustand().watch("settings", |app| {
            on_settings_changed(&app);
            Ok(())
        }) {
            Ok(_) => {
                app_log::info("settings watch installed");
                return;
            }
            Err(err) => {
                app_log::warn(&format!("settings watch failed: {err}"));
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// 创建托盘：图标按已选分类画电池；悬停 tip 同步；右键仅打开主界面/退出。双击打开窗口。
pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    app_log::info("setup_tray begin");
    let selected = read_stored_tray_device_id(app.handle());
    app.manage(battery::BatteryHub::new());
    app.manage(TrayBoard {
        samples: Mutex::new(HashMap::new()),
    });
    app.manage(SettingsWatch {
        snap: Mutex::new(SettingsSnap::default()),
    });

    let samples = HashMap::new();
    let menu = build_tray_menu(app.handle())?;
    let selected_by_category = read_selected_by_category(app.handle());
    let categories = read_tray_categories(app.handle());
    let levels = tray_visible_levels(&selected, &selected_by_category, &categories, &samples);
    let icon = make_tray_battery_icon(&levels);
    let tip = tooltip_text(&selected, &selected_by_category, &categories, &samples);

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        // 左键单击弹出菜单；双击打开主窗口
        .show_menu_on_left_click(true)
        .tooltip(tip)
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "show" => show_main_window(app),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            // Windows：双击打开主窗口
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    battery::spawn(app.handle().clone());
    app_log::info("setup_tray ok");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_label_covers_all_slots() {
        assert_eq!(category_label(DeviceCategory::Mouse), "鼠标");
        assert_eq!(category_label(DeviceCategory::Keyboard), "键盘");
        assert_eq!(category_label(DeviceCategory::Headset), "耳机");
    }

    #[test]
    fn tooltip_lists_only_selected_slots_and_marks_the_icon_device() {
        let mut samples = HashMap::new();
        samples.insert(
            "mchose-a5-v3-ultra-plus".into(),
            BatteryInfo {
                percent: Some(88),
                charging: None,
                connected: true,
                device: "迈从 A5 V3 Ultra+".into(),
                detail: String::new(),
                unsupported: false,
                model_uncertain: false,
            },
        );
        samples.insert(
            "razer-viper-ultimate".into(),
            BatteryInfo::disconnected("雷蛇毒蝰 Viper Ultimate", "未连接"),
        );
        // 只选了迈从鼠标 + VGN 键盘；未选的雷柏/雷蛇不应出现在 tip。
        let selected = SelectedByCategory {
            mouse: Some("mchose-a5-v3-ultra-plus".into()),
            keyboard: Some("vgn-s99".into()),
            headset: None,
        };
        let cats = sanitize_tray_categories(&["mouse".into(), "keyboard".into()]);
        let tooltip = tooltip_text("mchose-a5-v3-ultra-plus", &selected, &cats, &samples);
        assert!(
            tooltip.contains("✓ 迈从 A5 V3 Ultra+ · 88%"),
            "实际: {tooltip}"
        );
        assert!(tooltip.contains("VGN S99 · 读取中…"), "实际: {tooltip}");
        assert!(!tooltip.contains("VT9 AIR"), "实际: {tooltip}");
        assert!(!tooltip.contains("Viper Ultimate"), "实际: {tooltip}");
        assert_eq!(tooltip.lines().count(), 2, "实际: {tooltip}");
    }

    #[test]
    fn tooltip_fills_active_category_when_selection_empty() {
        let selected = SelectedByCategory::default();
        // 只勾选鼠标时，空 selection 仍会补上当前图标设备
        let cats = sanitize_tray_categories(&["mouse".into()]);
        let tooltip = tooltip_text("vt9-air", &selected, &cats, &HashMap::new());
        assert!(tooltip.contains("VT9 AIR"), "实际: {tooltip}");
        assert_eq!(tooltip.lines().count(), 1, "实际: {tooltip}");
        assert!(!tooltip.contains("VGN S99"), "实际: {tooltip}");
    }

    /// 托盘要把「充电无百分比」和「型号不支持」与「读失败」分开显示。
    ///
    /// review 意见 #2：这两种状态原先都落到「电量未知」，用户看不出区别；
    /// 图标还会按 0% 画，看起来像没电了。
    #[test]
    fn tray_distinguishes_charging_and_unsupported_from_read_failure() {
        // 充电哨兵：connected + charging + 无百分比
        let charging = BatteryInfo {
            percent: None,
            charging: Some(true),
            connected: true,
            device: "Gaming 2.4G".into(),
            detail: "正在充电".into(),
            unsupported: false,
            model_uncertain: true,
        };
        assert_eq!(battery_status(Some(&charging)), "充电中");
        // 充电时不画成 0%：图标走闪电形态
        assert_eq!(icon_mode(Some(&charging)), IconMode::Charging);

        // 型号不支持
        let unsupported = BatteryInfo::unsupported("前行者 S9 Ultra（未支持）", "本版只适配 S9 Pro");
        assert_eq!(battery_status(Some(&unsupported)), "型号不支持");
        assert_eq!(icon_mode(Some(&unsupported)), IconMode::Unsupported);

        // 真读失败（已连接、无电量、未充电）：仍是「电量未知」，不能和上面两种混
        let failed = BatteryInfo {
            percent: None,
            charging: None,
            connected: true,
            device: "Gaming 2.4G".into(),
            detail: "没读到回包".into(),
            unsupported: false,
            model_uncertain: false,
        };
        assert_eq!(battery_status(Some(&failed)), "电量未知");
        assert_eq!(icon_mode(Some(&failed)), IconMode::Level(None));

        // 有百分比的充电：继续显示百分比（作者明确要求）
        let charging_with_level = BatteryInfo {
            percent: Some(80),
            charging: Some(true),
            connected: true,
            device: "x".into(),
            detail: String::new(),
            unsupported: false,
            model_uncertain: false,
        };
        assert_eq!(battery_status(Some(&charging_with_level)), "80%");
        assert_eq!(
            icon_mode(Some(&charging_with_level)),
            IconMode::Level(Some(80))
        );

        // 未连接 / 无样本
        assert_eq!(
            battery_status(Some(&BatteryInfo::disconnected("x", "未连接"))),
            "未连接"
        );
        assert_eq!(battery_status(None), "读取中…");
    }

    #[test]
    fn battery_fill_rgb_matches_legacy_thresholds() {
        assert_eq!(battery_fill_rgb(Some(10)), (239, 68, 68));
        assert_eq!(battery_fill_rgb(Some(15)), (239, 68, 68));
        assert_eq!(battery_fill_rgb(Some(16)), (245, 158, 11));
        assert_eq!(battery_fill_rgb(Some(30)), (245, 158, 11));
        assert_eq!(battery_fill_rgb(Some(31)), (52, 211, 153));
        assert_eq!(battery_fill_rgb(None), (148, 163, 184));
    }

    #[test]
    fn tray_visible_levels_follow_category_filter() {
        let mut samples = HashMap::new();
        samples.insert(
            "mchose-a5-v3-ultra-plus".into(),
            BatteryInfo {
                percent: Some(12),
                charging: None,
                connected: true,
                device: "mchose".into(),
                detail: String::new(),
            
                unsupported: false,
                model_uncertain: false,
            },
        );
        samples.insert(
            "vgn-s99".into(),
            BatteryInfo {
                percent: Some(88),
                charging: None,
                connected: true,
                device: "vgn".into(),
                detail: String::new(),
            
                unsupported: false,
                model_uncertain: false,
            },
        );
        let selected = SelectedByCategory {
            mouse: Some("mchose-a5-v3-ultra-plus".into()),
            keyboard: Some("vgn-s99".into()),
            headset: None,
        };
        // 只勾选键盘 → 仅一格
        let cats = sanitize_tray_categories(&["keyboard".into()]);
        let levels = tray_visible_levels("mchose-a5-v3-ultra-plus", &selected, &cats, &samples);
        assert_eq!(levels, vec![Some(Some(88))]);

        // 勾选鼠+键 → 两格，跳过未启用耳机
        let cats = sanitize_tray_categories(&["mouse".into(), "keyboard".into()]);
        let levels = tray_visible_levels("mchose-a5-v3-ultra-plus", &selected, &cats, &samples);
        assert_eq!(levels, vec![Some(Some(12)), Some(Some(88))]);
    }

    #[test]
    fn sanitize_tray_categories_keeps_order_and_fallback() {
        let cats = sanitize_tray_categories(&["headset".into(), "mouse".into(), "nope".into()]);
        assert_eq!(
            cats,
            vec![DeviceCategory::Mouse, DeviceCategory::Headset]
        );
        assert_eq!(
            sanitize_tray_categories(&[]),
            vec![DeviceCategory::Mouse]
        );
    }

    #[test]
    fn make_tray_battery_icon_builds_32x32_for_1_2_3() {
        for n in 1..=3 {
            let slots: Vec<TraySlotLevel> = (0..n).map(|i| Some(Some((i * 30 + 10) as u8))).collect();
            let img = make_tray_battery_icon(&slots);
            assert_eq!(img.width(), 32, "n={n}");
            assert_eq!(img.height(), 32, "n={n}");
        }
        assert_eq!(vertical_battery_origins(2).len(), 2);
        assert_eq!(vertical_battery_origins(3).len(), 3);
    }

    #[test]
    fn tooltip_respects_tray_category_filter() {
        let selected = SelectedByCategory {
            mouse: Some("vt9-air".into()),
            keyboard: Some("vgn-s99".into()),
            headset: None,
        };
        let cats = sanitize_tray_categories(&["keyboard".into()]);
        let tip = tooltip_text("vt9-air", &selected, &cats, &HashMap::new());
        assert!(tip.contains("VGN S99"), "实际: {tip}");
        assert!(!tip.contains("VT9 AIR"), "实际: {tip}");
        assert_eq!(tip.lines().count(), 1);
    }

    #[test]
    fn unknown_or_wrong_category_ids_are_dropped() {
        let selected = SelectedByCategory {
            mouse: Some("missing-mouse".into()),
            keyboard: Some("vt9-air".into()),
            headset: None,
        };
        let clean = sanitize_selection(&selected);
        assert_eq!(clean.mouse, None);
        assert_eq!(clean.keyboard, None);
        assert_eq!(clean.headset, None);
    }

    #[test]
    fn explicit_mouse_selection_is_not_replaced_by_the_active_device() {
        let selected = SelectedByCategory {
            mouse: Some("razer-viper-ultimate".into()),
            keyboard: None,
            headset: None,
        };
        let resolved = resolve_selection("vt9-air", &selected);
        assert_eq!(resolved.mouse.as_deref(), Some("razer-viper-ultimate"));
    }
}
