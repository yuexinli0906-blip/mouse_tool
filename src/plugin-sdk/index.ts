/** 把共享 manifest 变成适配器。电量命令由壳层按 id 分发，插件不必各注册一条 IPC。 */
import type { DeviceAdapter, DeviceCategory } from './types'
import { commands } from '@/bindings'
import { DEVICE_CATEGORIES } from './types'

export type { BatteryInfo, DeviceAdapter, DeviceCategory, DeviceMeta } from './types'
export { DEVICE_CATEGORIES, DEVICE_CATEGORY_LABEL } from './types'

/** manifest.json 里的 category 是普通字符串，进适配器前收成已知分类。 */
function parseDeviceCategory(value: string): DeviceCategory {
  for (const category of DEVICE_CATEGORIES) {
    if (category === value) return category
  }
  throw new Error(`未知设备分类: ${value}`)
}

/**
 * 用 manifest 生成适配器。
 * `deviceId` 必须来自该文件，以便和 Rust `include_str!` 的同一份元数据对齐。
 */
export function createDeviceAdapter(meta: {
  id: string
  name: string
  description: string
  category: string
}): DeviceAdapter {
  const deviceId = meta.id
  return {
    id: meta.id,
    name: meta.name,
    description: meta.description,
    category: parseDeviceCategory(meta.category),
    async detect() {
      try {
        const info = await commands.getBattery(deviceId, null)
        return info.connected
      } catch {
        return false
      }
    },
    readBattery(options) {
      return commands.getBattery(deviceId, options?.fresh ?? false)
    },
  }
}
