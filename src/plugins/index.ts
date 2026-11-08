/** 已安装的前端设备插件。新增设备时在 `deviceRegistry` 追加一行。 */
import type { DeviceAdapter } from '@/plugin-sdk'
import { mchoseA5V3UltraPlus } from './mchose-a5-v3-ultra-plus'
import { qianxingzheS9Pro } from './qianxingzhe-s9-pro'
import { razerViperUltimate } from './razer-viper-ultimate'
import { vgnS99 } from './vgn-s99'
import { vt9Air } from './vt9-air'

export type { BatteryInfo, DeviceAdapter, DeviceCategory, DeviceMeta } from '@/plugin-sdk'
export { DEVICE_CATEGORIES, DEVICE_CATEGORY_LABEL } from '@/plugin-sdk'

/**
 * 首页与设置的设备列表。
 * 顺序应与 `src-tauri/src/devices/mod.rs` 的 `PLUGINS` 一致。
 * 元数据来自各插件的 `manifest.json`，不要在这里再写 id / 名称。
 */
export const deviceRegistry: DeviceAdapter[] = [
  vt9Air,
  razerViperUltimate,
  mchoseA5V3UltraPlus,
  qianxingzheS9Pro,
  vgnS99,
]

export function getDeviceById(id: string): DeviceAdapter | undefined {
  return deviceRegistry.find((device) => device.id === id)
}
