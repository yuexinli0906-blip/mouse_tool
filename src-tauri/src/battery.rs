//! 电量只读一次。托盘图标和首页都消费这次结果，不再各开一条 HID 读取。

use crate::app_log;
use crate::devices;
use crate::plugin_sdk::BatteryInfo;
use crate::tray;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// 一次正在进行或刚结束的 HID 读取。
/// `seq` 用来分清「等的是这一次」和「锁里已经被下一次读取换掉」。
struct Flight {
    seq: u64,
    device_id: String,
    /// 出发时的设备世代。读完如果世代变了，这次结果不能公布。
    generation: u64,
    done: bool,
    info: Option<BatteryInfo>,
}

/// 最近一次公布出去的采样。世代或设备对不上，就不能再拿来充数。
struct Cache {
    device_id: String,
    generation: u64,
    info: BatteryInfo,
    at: Instant,
}

struct Inner {
    flight: Option<Flight>,
    /// 每只设备最近一次采样。托盘提示要同时列出它们。
    samples: HashMap<String, Cache>,
    next_seq: u64,
}

pub struct BatteryHub {
    /// 进行中的读取和最近一次结果。HID 本身在锁外进行。
    inner: Mutex<Inner>,
    flight_cvar: Condvar,
    /// 和 `sleep_cvar` 配对。改标记和发通知必须拿着它，避免等待方刚好漏掉这次唤醒。
    sleep: Mutex<()>,
    sleep_cvar: Condvar,
    /// 为真时醒来要立刻再读。只改间隔或暂停时不置位，避免多读一次 HID。
    refresh: AtomicBool,
    /// 一切换设备就 +1。进行中的读取还拿着旧值。
    generation: AtomicU64,
}

impl BatteryHub {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                flight: None,
                samples: HashMap::new(),
                next_seq: 1,
            }),
            flight_cvar: Condvar::new(),
            sleep: Mutex::new(()),
            sleep_cvar: Condvar::new(),
            refresh: AtomicBool::new(false),
            generation: AtomicU64::new(1),
        }
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn lock_sleep(&self) -> std::sync::MutexGuard<'_, ()> {
        self.sleep.lock().unwrap_or_else(|err| err.into_inner())
    }

    /// 当前缓存是否还能代表这只设备。过了刷新间隔就要再读。
    pub(crate) fn cache_covers(&self, device_id: &str, max_age: Duration) -> bool {
        let inner = self.lock_inner();
        let generation = self.generation.load(Ordering::SeqCst);
        inner.samples.get(device_id).is_some_and(|cache| {
            cache_matches(cache.generation, generation, &cache.device_id, device_id)
                && sample_is_fresh(cache.at.elapsed(), max_age)
        })
    }
}

fn hub(app: &AppHandle) -> Option<tauri::State<'_, BatteryHub>> {
    app.try_state::<BatteryHub>()
}

/// 设备切了：进行中的读取结果作废，避免旧电量写到新设备上。
pub fn invalidate_device(app: &AppHandle) {
    if let Some(hub) = hub(app) {
        hub.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// 立刻再读一次。用于切换设备，以及从暂停恢复。
/// 手动刷新不走这里，它直接调用 `read_shared`，好和正在进行的那次合并。
pub fn request_refresh(app: &AppHandle) {
    if let Some(hub) = hub(app) {
        let _guard = hub.lock_sleep();
        hub.refresh.store(true, Ordering::SeqCst);
        hub.sleep_cvar.notify_one();
    }
}

/// 只打断等待，让循环用新的间隔或暂停状态重算，不额外读 HID。
pub fn poke(app: &AppHandle) {
    if let Some(hub) = hub(app) {
        let _guard = hub.lock_sleep();
        hub.sleep_cvar.notify_one();
    }
}

fn take_refresh(hub: &BatteryHub) -> bool {
    // 与 request_refresh 同一把锁，避免「清掉标记」和「置位并通知」交错。
    let _guard = hub.lock_sleep();
    hub.refresh.swap(false, Ordering::SeqCst)
}

/// 同一只设备、同一次切换里的并发调用共用这一次 HID 读取。
pub(crate) fn read_shared(app: &AppHandle, device_id: &str, fresh: bool) -> BatteryInfo {
    let Some(hub) = hub(app) else {
        return devices::read_battery(device_id);
    };

    let seq = loop {
        let mut inner = hub.lock_inner();
        let generation = hub.generation.load(Ordering::SeqCst);
        // 首页刚打开走非 fresh：缓存还对得上这只设备和这次切换就直接用，不再读 HID。
        // fresh 是周期采样和「立即刷新」，必须看到新数据。
        if !fresh {
            if let Some(cache) = inner.samples.get(device_id) {
                if cache_matches(cache.generation, generation, &cache.device_id, device_id) {
                    return cache.info.clone();
                }
            }
        }
        if let Some(flight) = &inner.flight {
            // 同一只、同一次切换：排进当前这次。另一只还在读：先等它结束，HID 不能并行。
            let join = !flight.done
                && flight.device_id == device_id
                && flight.generation == generation;
            if join || !flight.done {
                let flight_seq = flight.seq;
                while inner
                    .flight
                    .as_ref()
                    .is_some_and(|flight| flight.seq == flight_seq && !flight.done)
                {
                    inner = hub
                        .flight_cvar
                        .wait(inner)
                        .unwrap_or_else(|err| err.into_inner());
                }
                if join {
                    if let Some(info) = inner
                        .flight
                        .as_ref()
                        .filter(|flight| flight.seq == flight_seq && flight.done)
                        .and_then(|flight| flight.info.clone())
                    {
                        return info;
                    }
                }
                continue;
            }
        }

        let generation = hub.generation.load(Ordering::SeqCst);
        let seq = inner.next_seq;
        inner.next_seq = inner.next_seq.wrapping_add(1);
        inner.flight = Some(Flight {
            seq,
            device_id: device_id.to_string(),
            generation,
            done: false,
            info: None,
        });
        break seq;
    };

    // 锁外读 HID。代际变了说明读的过程中换过设备，这次结果丢掉。
    // 不是图标当前那一只的结果也要留下，托盘提示要列出每一只。
    let raw = devices::read_battery(device_id);
    let (publish, info) = {
        let mut inner = hub.lock_inner();
        let now_gen = hub.generation.load(Ordering::SeqCst);
        let flight_gen = inner
            .flight
            .as_ref()
            .filter(|flight| flight.seq == seq)
            .map(|flight| flight.generation);
        let publish = flight_gen.is_some_and(|generation| should_publish(generation, now_gen));
        // 读空电量时沿用该设备上次有效值（休眠常见），断连 / 不支持不沿用。
        let info = if publish {
            match inner.samples.get(device_id) {
                Some(cache) => keep_last_percent(&cache.info, raw.clone()),
                None => raw.clone(),
            }
        } else {
            raw.clone()
        };
        if let Some(flight) = inner.flight.as_mut().filter(|flight| flight.seq == seq) {
            flight.done = true;
            flight.info = Some(info.clone());
        }
        if publish {
            inner.samples.insert(
                device_id.to_string(),
                Cache {
                    device_id: device_id.to_string(),
                    generation: now_gen,
                    info: info.clone(),
                    at: Instant::now(),
                },
            );
        }
        hub.flight_cvar.notify_all();
        (publish, info)
    };

    if publish {
        tray::publish_battery(app, device_id, &info);
        emit_battery(app, device_id, &info);
    }
    info
}

/// 首页订阅的电量推送。事件名是 `battery-updated`。
#[derive(Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "battery-updated")]
pub struct BatteryUpdated {
    device_id: String,
    percent: Option<u8>,
    charging: Option<bool>,
    connected: bool,
    device: String,
    detail: String,
}

fn emit_battery(app: &AppHandle, device_id: &str, info: &BatteryInfo) {
    let payload = BatteryUpdated {
        device_id: device_id.to_string(),
        percent: info.percent,
        charging: info.charging,
        connected: info.connected,
        device: info.device.clone(),
        detail: info.detail.clone(),
    };
    if let Err(err) = tauri_specta::Event::emit(&payload, app) {
        app_log::warn(&format!("battery event emit failed: {err}"));
    }
}

/// 当前图标设备先读，其余设备按插件表跟在后面。
pub(crate) fn sample_order(active_id: &str) -> Vec<String> {
    let mut ids = Vec::new();
    if !active_id.is_empty() {
        ids.push(active_id.to_string());
    }
    for meta in devices::list_meta() {
        if meta.id != active_id {
            ids.push(meta.id);
        }
    }
    ids
}

/// 任何一只过期或还没读过，就马上再采。都还新鲜时，睡到最早过期的那一只。
fn cache_wait(hub: &BatteryHub, interval: Duration) -> Duration {
    let inner = hub.lock_inner();
    let generation = hub.generation.load(Ordering::SeqCst);
    let metas = devices::list_meta();
    if metas.is_empty() {
        return interval;
    }
    let mut wait = interval;
    for meta in metas {
        match inner.samples.get(&meta.id) {
            Some(cache)
                if cache_matches(cache.generation, generation, &cache.device_id, &meta.id)
                    && sample_is_fresh(cache.at.elapsed(), interval) =>
            {
                wait = wait.min(interval.saturating_sub(cache.at.elapsed()));
            }
            _ => return Duration::ZERO,
        }
    }
    wait
}

fn run_loop(app: AppHandle) {
    'sample: loop {
        let Some(state) = hub(&app) else {
            return;
        };
        let refresh = take_refresh(&state);
        let generation = state.generation.load(Ordering::SeqCst);
        drop(state);

        let auto = tray::auto_refresh_enabled(&app);
        // 暂停只停周期采样。换设备仍然读一次，否则图标会停在上一只上。
        // 缓存还在间隔内，并且没有人要求立刻读，就睡到剩余时间，不重复开 HID。
        if refresh || auto {
            let active = tray::read_stored_tray_device_id(&app);
            let interval = Duration::from_secs(tray::auto_interval_sec(&app));
            for id in sample_order(&active) {
                let Some(state) = hub(&app) else {
                    return;
                };
                if state.generation.load(Ordering::SeqCst) != generation {
                    continue 'sample;
                }
                let covered = state.cache_covers(&id, interval);
                drop(state);
                if refresh || !covered {
                    let _info = read_shared(&app, &id, true);
                }
                let Some(state) = hub(&app) else {
                    return;
                };
                // 读的过程中换了图标设备：丢掉这一轮，从新的那只重新开始。
                if state.generation.load(Ordering::SeqCst) != generation {
                    continue 'sample;
                }
            }
        }

        let Some(state) = hub(&app) else {
            return;
        };
        if !tray::auto_refresh_enabled(&app) {
            let guard = state.lock_sleep();
            // 拿着锁再看一次。request_refresh 也拿这把锁，所以不会在检查之后、睡下去之前把通知漏掉。
            if state.refresh.load(Ordering::SeqCst) {
                continue;
            }
            let _guard = state
                .sleep_cvar
                .wait(guard)
                .unwrap_or_else(|err| err.into_inner());
            continue;
        }

        let interval = Duration::from_secs(tray::auto_interval_sec(&app));
        let wait = cache_wait(&state, interval);
        if wait.is_zero() {
            continue;
        }
        let guard = state.lock_sleep();
        if state.refresh.load(Ordering::SeqCst) {
            continue;
        }
        let _guard = state
            .sleep_cvar
            .wait_timeout(guard, wait)
            .unwrap_or_else(|err| err.into_inner());
    }
}

/// 等设置 store 出现后再盯着它，避免启动时用默认设备读完又立刻重读。
pub fn spawn(app: AppHandle) {
    thread::spawn(move || {
        tray::install_settings_watch(&app);
        run_loop(app);
    });
}

/// 壳层命令。`fresh` 为真时绕过缓存再读一次；省略时返回当前这次采样。
#[tauri::command]
#[specta::specta]
pub async fn get_battery(app: AppHandle, device_id: String, fresh: Option<bool>) -> BatteryInfo {
    let fresh = fresh.unwrap_or(false);
    let label = devices::device_name(&device_id);
    match tauri::async_runtime::spawn_blocking(move || read_shared(&app, &device_id, fresh)).await
    {
        Ok(info) => info,
        Err(err) => BatteryInfo::disconnected(label, format!("读取任务失败: {err}")),
    }
}


/// 休眠等场景会读到「已连接但 percent 为空」。保留该设备上次有效百分比，
/// 托盘与首页都走同一份结果，避免界面被刷成「--」。
pub(crate) fn keep_last_percent(prev: &BatteryInfo, next: BatteryInfo) -> BatteryInfo {
    if next.percent.is_some() || next.unsupported || !next.connected {
        return next;
    }
    match prev.percent {
        Some(p) if prev.connected && !prev.unsupported => BatteryInfo {
            percent: Some(p),
            ..next
        },
        _ => next,
    }
}

pub(crate) fn cache_matches(
    cache_generation: u64,
    now_generation: u64,
    cache_id: &str,
    want_id: &str,
) -> bool {
    cache_generation == now_generation && cache_id == want_id
}

pub(crate) fn sample_is_fresh(age: Duration, interval: Duration) -> bool {
    age < interval
}

/// 代际没变，这次结果就属于被读的那只设备，可以写进托盘提示。
/// 图标数字是否跟着更新，由托盘再看是不是当前选中的那一只。
pub(crate) fn should_publish(flight_generation: u64, now_generation: u64) -> bool {
    flight_generation == now_generation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_read_is_not_published() {
        assert!(should_publish(1, 1));
        assert!(!should_publish(1, 2));
    }

    #[test]
    fn sample_order_reads_the_icon_device_first() {
        // 不写死设备清单：先读当前选中的那只，再按 PLUGINS 顺序补齐其余。
        // 这样以后再加设备时这条测试不会因为「多了一只」而红。
        let order = sample_order("mchose-a5-v3-ultra-plus");
        assert_eq!(order.first().map(String::as_str), Some("mchose-a5-v3-ultra-plus"));
        assert_eq!(order.len(), devices::list_meta().len());

        let mut sorted_registered: Vec<_> = devices::list_meta().into_iter().map(|m| m.id).collect();
        sorted_registered.sort();
        let mut sorted_order = order.clone();
        sorted_order.sort();
        assert_eq!(sorted_order, sorted_registered, "sample_order 应覆盖全部已注册设备");

        // 再确认一次顺序：第一项之后应严格按 PLUGINS 顺序，且不重复当前设备。
        let expected: Vec<String> = devices::list_meta()
            .into_iter()
            .map(|m| m.id)
            .filter(|id| id != "mchose-a5-v3-ultra-plus")
            .collect();
        assert_eq!(&order[1..], expected.as_slice());
    }

    #[test]
    fn sleep_read_keeps_previous_percent() {
        let prev = BatteryInfo {
            percent: Some(76),
            charging: Some(false),
            connected: true,
            device: "毒蝰".into(),
            detail: "ok".into(),
            unsupported: false,
            model_uncertain: false,
        };
        let asleep = BatteryInfo {
            percent: None,
            charging: None,
            connected: true,
            device: "毒蝰".into(),
            detail: "等待唤醒".into(),
            unsupported: false,
            model_uncertain: false,
        };
        let kept = keep_last_percent(&prev, asleep.clone());
        assert_eq!(kept.percent, Some(76));
        assert_eq!(kept.detail, "等待唤醒");

        let gone = BatteryInfo::disconnected("毒蝰", "未找到接收器");
        assert!(keep_last_percent(&prev, gone).percent.is_none());

        let unsupported = BatteryInfo::unsupported("毒蝰", "不支持");
        assert!(keep_last_percent(&prev, unsupported).percent.is_none());

        let fresh = BatteryInfo {
            percent: Some(50),
            ..asleep
        };
        assert_eq!(keep_last_percent(&prev, fresh).percent, Some(50));
    }

    #[test]
    fn cache_from_another_device_or_generation_is_not_reused() {
        assert!(cache_matches(3, 3, "vt9-air", "vt9-air"));
        assert!(!cache_matches(3, 4, "vt9-air", "vt9-air"));
        assert!(!cache_matches(3, 3, "vt9-air", "razer-viper-ultimate"));
        assert!(sample_is_fresh(Duration::from_secs(2), Duration::from_secs(3)));
        assert!(!sample_is_fresh(
            Duration::from_secs(3),
            Duration::from_secs(3)
        ));
    }
}
