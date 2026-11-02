import type { BatteryInfo, DeviceCategory as SpectaDeviceCategory } from '@/bindings'

/** 电量与设备元数据来自 Rust，经 `src/bindings.ts` 导出。`unsupported` / `model_uncertain` 缺省等同 false。 */
export type { BatteryInfo, DeviceMeta } from '@/bindings'

/** 一级分类。顺序即首页分类栏顺序，与 Rust `DeviceCategory` 一致。 */
export const DEVICE_CATEGORIES = ['mouse', 'keyboard', 'headset'] as const satisfies readonly SpectaDeviceCategory[]

export type DeviceCategory = SpectaDeviceCategory

export const DEVICE_CATEGORY_LABEL: Record<DeviceCategory, string> = {
  mouse: '鼠标',
  keyboard: '键盘',
  headset: '耳机',
}

export interface DeviceAdapter {
  /** 稳定 id，与 Rust 插件 manifest 的 `id` 相同 */
  id: string
  /** 界面显示名称 */
  name: string
  /** 列表里的短说明 */
  description: string
  /** 一级分类，来自 manifest */
  category: DeviceCategory
  /** 当前是否值得尝试读电量 */
  detect: () => Promise<boolean>
  /**
   * 读电量。走壳层 `get_battery(deviceId)`。
   * `fresh` 为真时绕过缓存再读一次；否则返回这只设备最近一次采样（和托盘提示同一份）。
   */
  readBattery: (options?: { fresh?: boolean }) => Promise<BatteryInfo>
}
