//! 设备插件契约（构建期，不是动态库）。
//!
//! 每只设备提供一份与前端共用的 `manifest.json`，外加一个电量函数。
//! 壳层用 `get_battery(device_id)` 按 id 分发；设置、更新、日志不属于插件。

use serde::{Deserialize, Serialize};

/// 一级分类。与前端 `DeviceCategory` 同一套取值：`mouse` / `keyboard` / `headset`。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DeviceCategory {
    Mouse,
    Keyboard,
    Headset,
}

/// 前端 `DeviceMeta` 与 `manifest.json` 的同一份字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct DeviceMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: DeviceCategory,
}

/// 电量结果。字段名与前端 `BatteryInfo` 一致。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default, specta::Type)]
pub struct BatteryInfo {
    pub percent: Option<u8>,
    pub charging: Option<bool>,
    pub connected: bool,
    pub device: String,
    pub detail: String,
    /// 已连接，但这只设备本身不受该插件支持（例如插的是同系列其它型号）。
    ///
    /// 与「读不到电量」区分开：前者用户重插/开机都没用，界面不该给排障提示。
    /// `#[serde(default)]` 让旧插件与旧数据不用改；序列化时总是带上（false）。
    #[serde(default)]
    pub unsupported: bool,
    /// 已连接、电量也读到了，但**具体型号认不出来**（例如 2.4G 接收器 PID
    /// 被同系列多型号共用）。主界面要如实说明，不能把型号说死。
    /// `#[serde(default)]` 同 `unsupported`。
    #[serde(default)]
    pub model_uncertain: bool,
}

impl BatteryInfo {
    pub fn disconnected(device: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            detail: detail.into(),
            ..Default::default()
        }
    }

    /// 已连接，但型号不受支持。界面上要把原因说清楚，不能只说「读不到电量」。
    pub fn unsupported(device: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            connected: true,
            device: device.into(),
            detail: detail.into(),
            unsupported: true,
            ..Default::default()
        }
    }
}

/// 编译期注册的一只鼠标。
pub struct DevicePlugin {
    /// `include_str!` 进来的 manifest，与前端插件目录是同一个文件。
    pub manifest_json: &'static str,
    /// 读取电量。默认只做中断读。
    /// Feature 报告只允许模块文档写明 opt-in 的插件使用（雷蛇）。
    /// 雷柏 VT9 禁止 Feature / `GetInputReport`。
    pub read_battery: fn() -> Result<BatteryInfo, String>,
}

impl DevicePlugin {
    pub fn meta(&self) -> DeviceMeta {
        parse_manifest(self.manifest_json)
    }
}

pub fn parse_manifest(manifest_json: &str) -> DeviceMeta {
    serde_json::from_str(manifest_json)
        .unwrap_or_else(|err| panic!("设备插件 manifest 无法解析: {err}\n{manifest_json}"))
}
