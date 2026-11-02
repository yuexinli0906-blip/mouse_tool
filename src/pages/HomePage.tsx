/** 首页：左侧设备侧栏，右侧电量 hero 卡。电量来自后端同一次采样，不自己再读 HID。 */
import type { BatteryInfo } from '@/devices'
import { useEffect, useRef, useState } from 'react'
import { commands, events } from '@/bindings'
import { BatteryCard } from '@/components/BatteryCard'
import { DeviceList } from '@/components/DeviceList'
import { deviceRegistry, getDeviceById } from '@/devices'
import { resolveSelectedByCategory, useSettingsStore } from '@/stores/settings'

interface Sample {
  deviceId: string
  info: BatteryInfo
  at: Date
}

export function HomePage() {
  const autoIntervalSec = useSettingsStore((s) => s.autoIntervalSec)
  const autoRefresh = useSettingsStore((s) => s.autoRefresh)
  const trayDeviceId = useSettingsStore((s) => s.trayDeviceId)
  const storedSelection = useSettingsStore((s) => s.selectedByCategory)
  const setTrayDeviceId = useSettingsStore((s) => s.setTrayDeviceId)
  const setAutoRefresh = useSettingsStore((s) => s.setAutoRefresh)
  const selected = getDeviceById(trayDeviceId) ?? deviceRegistry[0]
  const selectedByCategory = resolveSelectedByCategory(selected?.id ?? '', storedSelection)
  const [sample, setSample] = useState<Sample | null>(null)
  const [error, setError] = useState<{ deviceId: string; message: string } | null>(null)
  const [refreshingId, setRefreshingId] = useState<string | null>(null)
  // 样本带着读它时的设备 id。换设备后对不上就先空白，避免上一只的电量留在卡片上。
  const matching = sample?.deviceId === selected?.id ? sample : null
  const visibleError = error?.deviceId === selected?.id ? error.message : null
  const loading = refreshingId === selected?.id || (matching == null && visibleError == null)

  function applySample(deviceId: string, info: BatteryInfo) {
    setSample({ deviceId, info, at: new Date() })
    setError((current) => (current?.deviceId === deviceId ? null : current))
  }
  // 渲染时更新。点击换设备会先改 store 再重渲染，事件回来时这里已经是新 id。
  // 监听不跟着设备重订，否则退订到重订的空隙里事件会丢，卡片就要空等到下一轮间隔。
  const selectedIdRef = useRef(selected?.id)
  useEffect(() => {
    selectedIdRef.current = selected?.id
  })

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | undefined
    void events.batteryUpdated.listen((event) => {
      if (cancelled || event.payload.deviceId !== selectedIdRef.current) return
      const { deviceId, ...info } = event.payload
      setSample({ deviceId, info, at: new Date() })
      setError((current) => (current?.deviceId === deviceId ? null : current))
    }).then((stop) => {
      if (cancelled) stop()
      else unlisten = stop
    })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  // 只在进页面时要一次当前采样。有缓存就不再读 HID。
  // 换设备不在这里读，否则会和托盘那次采样各开一条 HID。
  useEffect(() => {
    const deviceId = selectedIdRef.current
    const device = deviceId ? (getDeviceById(deviceId) ?? deviceRegistry[0]) : undefined
    if (!device) return
    let cancelled = false
    void device
      .readBattery()
      .then((info) => {
        if (!cancelled && selectedIdRef.current === device.id) {
          setSample({ deviceId: device.id, info, at: new Date() })
          setError((current) => (current?.deviceId === device.id ? null : current))
        }
      })
      .catch((e: unknown) => {
        if (!cancelled && selectedIdRef.current === device.id)
          setError({ deviceId: device.id, message: String(e) })
      })
    return () => {
      cancelled = true
    }
  }, [])

  async function refresh() {
    if (!selected) return
    const deviceId = selected.id
    setRefreshingId(deviceId)
    setError((current) => (current?.deviceId === deviceId ? null : current))
    try {
      const info = await selected.readBattery({ fresh: true })
      // 读取期间用户已切到另一只：丢弃本次结果，等新设备的 battery-updated / 下一轮。
      if (useSettingsStore.getState().trayDeviceId !== trayDeviceId) return
      if (
        (getDeviceById(useSettingsStore.getState().trayDeviceId) ?? deviceRegistry[0])?.id !==
        deviceId
      )
        return
      applySample(deviceId, info)
    } catch (e) {
      if (
        (getDeviceById(useSettingsStore.getState().trayDeviceId) ?? deviceRegistry[0])?.id !==
        deviceId
      )
        return
      setError({ deviceId, message: String(e) })
    } finally {
      setRefreshingId((current) => (current === deviceId ? null : current))
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col md:flex-row">
      <DeviceList
        devices={deviceRegistry}
        activeId={selected.id}
        selectedByCategory={selectedByCategory}
        onSelect={(id) => {
          setTrayDeviceId(id)
          setAutoRefresh(true)
          const next = useSettingsStore.getState().selectedByCategory
          void commands.setTrayDevice(id, next).catch((e: unknown) => {
            console.error('set_tray_device failed', e)
          })
        }}
        auto={autoRefresh}
        autoIntervalSec={autoIntervalSec}
      />

      <main className="relative min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto">
        <div className="bg-primary/10 pointer-events-none absolute -top-40 right-[-12%] size-130 rounded-full blur-[120px]" />
        <div className="relative mx-auto flex min-h-full w-full max-w-215 flex-col justify-start px-5 py-6 lg:justify-center lg:px-10 lg:py-8">
          {selected ? (
            <BatteryCard
              deviceName={selected.name}
              info={matching?.info ?? null}
              loading={loading}
              auto={autoRefresh}
              autoIntervalSec={autoIntervalSec}
              error={visibleError}
              lastUpdated={matching?.at ?? null}
              onRefresh={refresh}
              onStartRefresh={() => setAutoRefresh(true)}
              onStopRefresh={() => setAutoRefresh(false)}
            />
          ) : (
            <div className="flex flex-1 items-center justify-center">
              <p className="text-muted-foreground">尚未注册任何设备</p>
            </div>
          )}
        </div>
      </main>
    </div>
  )
}
