//! 已安装的设备插件。
//!
//! 新增设备：加上模块，并在 [`PLUGINS`] 追加一行。
//! 托盘和 `get_battery` 都读这张表，不要再维护第二份设备列表。

mod mchose_a5_v3_ultra;
pub(crate) mod qianxingzhe_s9_pro;
mod razer_viper_ultimate;
mod vgn_s99;
mod vt9_air;

use crate::plugin_sdk::{BatteryInfo, DeviceMeta, DevicePlugin};

/// 构建期设备表。顺序即默认设备和托盘菜单的顺序。托盘列出表里的每一只。
/// 按分类分组排列（鼠标 → 键盘 → 耳机），与托盘菜单分组顺序、前端 `deviceRegistry` 保持一致。
static PLUGINS: &[&DevicePlugin] = &[
    &vt9_air::PLUGIN,
    &razer_viper_ultimate::PLUGIN,
    &mchose_a5_v3_ultra::PLUGIN,
    &qianxingzhe_s9_pro::PLUGIN,
    &vgn_s99::PLUGIN,
];

pub fn find(device_id: &str) -> Option<&'static DevicePlugin> {
    PLUGINS
        .iter()
        .copied()
        .find(|plugin| plugin.meta().id == device_id)
}

pub fn default_device_id() -> String {
    PLUGINS
        .first()
        .map(|plugin| plugin.meta().id)
        .unwrap_or_default()
}

pub fn list_meta() -> Vec<DeviceMeta> {
    PLUGINS.iter().map(|plugin| plugin.meta()).collect()
}

pub fn device_name(device_id: &str) -> String {
    find(device_id)
        .map(|plugin| plugin.meta().name)
        .unwrap_or_else(|| device_id.to_string())
}

pub fn read_battery(device_id: &str) -> BatteryInfo {
    let Some(plugin) = find(device_id) else {
        return BatteryInfo::disconnected(device_id, format!("未知设备: {device_id}"));
    };
    let meta = plugin.meta();
    match (plugin.read_battery)() {
        Ok(info) => info,
        Err(detail) => BatteryInfo::disconnected(meta.name, detail),
    }
}

/// 壳层命令：返回已注册设备。元数据与前端 manifest 同源。
#[tauri::command]
pub fn list_devices() -> Vec<DeviceMeta> {
    list_meta()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_sdk::DeviceCategory;

    #[test]
    fn plugin_ids_are_unique_and_match_shared_manifest() {
        let metas = list_meta();
        assert!(!metas.is_empty(), "至少注册一只鼠标");

        let mut ids: Vec<_> = metas.iter().map(|meta| meta.id.clone()).collect();
        let len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), len, "插件 id 重复");

        assert_eq!(metas[0].id, "vt9-air");
        assert_eq!(metas[0].name, "VT9 AIR");
        assert_eq!(metas[0].description, "雷柏 VT9 AIR 无线鼠标");
        assert_eq!(metas[0].category, DeviceCategory::Mouse);
        assert_eq!(default_device_id(), "vt9-air");
        assert_eq!(metas[1].id, "razer-viper-ultimate");
        assert_eq!(metas[1].name, "毒蝰终极版 Viper Ultimate");
        assert_eq!(metas[1].category, DeviceCategory::Mouse);
        assert_eq!(metas[2].id, "mchose-a5-v3-ultra-plus");
        assert_eq!(metas[2].name, "迈从 A5 V3 Ultra+");
        assert_eq!(metas[2].category, DeviceCategory::Mouse);
        // 前行者 S9 Pro（第 4 只鼠标）
        assert_eq!(metas[3].id, "qianxingzhe-s9-pro");
        assert_eq!(metas[3].name, "前行者 S9 Pro");
        assert_eq!(metas[3].description, "2.4G 无线游戏鼠标");
        assert_eq!(metas[3].category, DeviceCategory::Mouse);
        // 首只键盘设备
        assert_eq!(metas[4].id, "vgn-s99");
        assert_eq!(metas[4].name, "VGN S99");
        assert_eq!(metas[4].description, "2.4G 无线机械键盘");
        assert_eq!(metas[4].category, DeviceCategory::Keyboard);
        assert!(find("vt9-air").is_some());
        assert!(find("razer-viper-ultimate").is_some());
        assert!(find("mchose-a5-v3-ultra-plus").is_some());
        assert!(find("vgn-s99").is_some());
        assert!(find("qianxingzhe-s9-pro").is_some());
        assert!(find("missing-mouse").is_none());
    }

    #[test]
    fn unknown_device_does_not_panic() {
        let info = read_battery("missing-mouse");
        assert!(!info.connected);
        assert_eq!(info.device, "missing-mouse");
        assert!(info.detail.contains("未知设备"));
        assert!(info.percent.is_none());
        // 未知设备不是「型号不支持」——那是「设备在、但不属于本插件」的专门状态。
        assert!(!info.unsupported);
    }

    /// `unsupported` 与 `disconnected` 必须是两个不同状态。
    ///
    /// 前端靠它决定文案：`unsupported` 要直说「该型号暂不支持」并显示具体型号；
    /// `disconnected` 才提示去检查接收器/开机。混用会让用户对着不支持的型号
    /// 反复拔插设备，问题永远解决不了。
    #[test]
    fn unsupported_is_not_disconnected() {
        let info = BatteryInfo::unsupported("前行者 S9 Ultra（未支持）", "本版只适配 S9 Pro");
        assert!(info.unsupported, "unsupported() 必须置 unsupported=true");
        assert!(
            info.connected,
            "设备其实插着，不能报未连接，否则界面会引导用户去查线"
        );
        assert!(info.percent.is_none());
        assert!(info.detail.contains("S9 Pro"));

        let gone = BatteryInfo::disconnected("前行者 S9 Pro", "未找到接收器");
        assert!(!gone.unsupported);
        assert!(!gone.connected);
    }

    /// 实机验证：需要前行者 S9 Pro 接收器插在本机。
    ///
    /// 默认不跑（`#[ignore]`），因为它依赖硬件。手动执行：
    ///
    /// ```bash
    /// cargo test --lib s9_pro_reads_real_hardware -- --ignored --nocapture
    /// ```
    ///
    /// 断言的是「插件自己的代码路径能读到电量」，不是探针的结果。
    /// 连续读 3 次，同时验证稳定性——单次成功可能是设备恰好回了一次，
    /// 3 次都在合理区间才说明不是偶然。
    #[test]
    #[ignore = "需要插着前行者 S9 Pro 接收器"]
    fn s9_pro_reads_real_hardware() {
        const ROUNDS: usize = 3;
        let mut levels = Vec::new();

        for round in 1..=ROUNDS {
            let info = read_battery("qianxingzhe-s9-pro");
            println!(
                "第 {round}/{ROUNDS} 次: connected={} percent={:?} charging={:?} device={} detail={}",
                info.connected, info.percent, info.charging, info.device, info.detail
            );

            assert!(
                info.connected,
                "第 {round} 次：接收器插着时应报已连接: {}",
                info.detail
            );
            match info.percent {
                // 正常电量：必须是合法百分比
                Some(percent) => {
                    assert!(percent <= 100, "第 {round} 次：电量 {percent} 超过 100");
                    levels.push(percent);
                }
                // 充电中：厂商哨兵值，拿不到百分比是正常的
                None => assert_eq!(
                    info.charging,
                    Some(true),
                    "第 {round} 次：没电量的唯一合法原因是充电中，否则就是读取失败: {}",
                    info.detail
                ),
            }
        }

        // 未充电时必须每次都能读到电量，不能时有时无。
        if !levels.is_empty() {
            assert_eq!(
                levels.len(),
                ROUNDS,
                "只读到 {}/{ROUNDS} 次电量，读取不稳定: {levels:?}",
                levels.len()
            );
            // 一分钟内电量最多掉 1~2 个百分点；波动过大说明解析位置或协议读错了。
            let max = *levels.iter().max().unwrap();
            let min = *levels.iter().min().unwrap();
            assert!(
                max - min <= 5,
                "连续读数波动过大（{min}..{max}），电量解析不可信: {levels:?}"
            );
            println!("连续 {ROUNDS} 次读数: {levels:?}（波动 {} 点）", max - min);
        }
    }
}
