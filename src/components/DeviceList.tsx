/** 设备侧栏：品牌区、分类切换、设备列表和自动刷新状态。 */
import type { DeviceAdapter, DeviceCategory } from '@/devices'
import { Icon } from '@iconify/react'
import { useEffect, useState } from 'react'
import { MorphBot } from '@/components/MorphBot'
import { DEVICE_CATEGORIES, DEVICE_CATEGORY_LABEL } from '@/devices'
import { cn } from '@/lib/utils'

const CATEGORY_ICON: Record<DeviceCategory, string> = {
  mouse: 'mdi:mouse',
  keyboard: 'mdi:keyboard',
  headset: 'mdi:headphones',
}

interface Props {
  devices: DeviceAdapter[]
  /** 电量卡片 / 托盘图标当前跟着哪一只 */
  activeId: string
  /** 各分类里选中的那一只，用来画高亮 */
  selectedByCategory: Record<DeviceCategory, string | null>
  onSelect: (id: string) => void
  onCategoryChange: (category: DeviceCategory) => void
  auto: boolean
  autoIntervalSec: number
}

export function DeviceList({
  devices,
  activeId,
  selectedByCategory,
  onSelect,
  onCategoryChange,
  auto,
  autoIntervalSec,
}: Props) {
  const [category, setCategory] = useState<DeviceCategory>(() => {
    return devices.find(device => device.id === activeId)?.category ?? 'mouse'
  })
  const selectedId = selectedByCategory[category]
  const visible = devices.filter(device => device.category === category)
  const activeCategory = devices.find(device => device.id === activeId)?.category

  // 托盘/设置里换了设备时，侧栏也跟着切到对应分类。
  useEffect(() => {
    if (activeCategory) {
      // 外部（托盘/设置）切换设备时同步分类，这是有意的 props → state 同步。
      // eslint-disable-next-line react/set-state-in-effect
      setCategory(activeCategory)
    }
  }, [activeCategory])

  return (
    <aside className="border-border/70 bg-card/30 flex w-full shrink-0 flex-col border-b md:h-full md:w-[280px] md:border-r md:border-b-0">
      <div className="flex items-center gap-3 px-5 pt-5 pb-4">
        <div className="bg-primary/10 ring-primary/10 flex size-16 shrink-0 items-center justify-center rounded-2xl ring-1">
          <MorphBot size={52} />
        </div>
        <div className="min-w-0">
          <p className="text-sm font-semibold tracking-tight">外设电量</p>
          <p className="text-muted-foreground mt-0.5 text-[11px]">yugutou_tool</p>
        </div>
      </div>

      <div className="px-3">
        <div
          role="tablist"
          aria-label="设备分类"
          className="bg-muted/40 border-border/60 grid grid-cols-3 gap-1 rounded-xl border p-1"
        >
          {DEVICE_CATEGORIES.map((id) => {
            const active = id === category
            return (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => {
                  setCategory(id)
                  onCategoryChange(id)
                }}
                className={cn(
                  'flex items-center justify-center gap-1.5 rounded-lg px-1.5 py-1.5 text-[11px] font-medium transition-all',
                  active
                    ? 'bg-primary/15 text-primary border-primary/25 border shadow-sm'
                    : 'text-muted-foreground hover:text-foreground hover:bg-background/40 border border-transparent',
                )}
              >
                <Icon icon={CATEGORY_ICON[id]} className="size-3.5" />
                {DEVICE_CATEGORY_LABEL[id]}
              </button>
            )
          })}
        </div>
      </div>

      <div className="mt-5 flex min-h-0 flex-1 flex-col">
        <div className="flex items-center justify-between px-5 pb-2">
          <p className="text-muted-foreground text-[10px] font-medium tracking-[0.18em] uppercase">
            设备
          </p>
          <span className="text-muted-foreground text-[10px] tabular-nums">
            {visible.length}
          </span>
        </div>

        <div className="min-h-0 flex-1 overflow-auto px-3 pb-3">
          {visible.length === 0
            ? (
                <div className="border-border/60 flex flex-col items-center gap-2 rounded-2xl border border-dashed px-4 py-8 text-center">
                  <Icon
                    icon={CATEGORY_ICON[category]}
                    className="text-muted-foreground/50 size-6"
                  />
                  <p className="text-muted-foreground text-xs">
                    {`暂无${DEVICE_CATEGORY_LABEL[category]}`}
                  </p>
                </div>
              )
            : (
                <ul className="flex flex-col gap-1.5">
                  {visible.map((device) => {
                    const selected = device.id === selectedId
                    return (
                      <li key={device.id}>
                        <button
                          type="button"
                          title={device.description}
                          onClick={() => onSelect(device.id)}
                          className={cn(
                            'group relative flex w-full items-center gap-3 rounded-2xl border px-3 py-2.5 text-left transition-all',
                            selected
                              ? 'border-primary/35 bg-primary/10'
                              : 'border-transparent hover:border-border/60 hover:bg-accent/50',
                          )}
                        >
                          {selected && (
                            <span className="bg-primary absolute inset-y-2 left-0 w-0.5 rounded-full" />
                          )}
                          <span
                            className={cn(
                              'flex size-9 shrink-0 items-center justify-center rounded-xl transition-colors',
                              selected
                                ? 'bg-primary/15 text-primary'
                                : 'bg-muted/60 text-muted-foreground group-hover:text-foreground',
                            )}
                          >
                            <Icon icon={CATEGORY_ICON[device.category]} className="size-4" />
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="flex items-center gap-1.5">
                              <span className="truncate text-sm font-medium">
                                {device.name}
                              </span>
                              {selected && (
                                <Icon
                                  icon="mdi:check-circle"
                                  className="text-primary size-3.5 shrink-0"
                                />
                              )}
                            </span>
                            <span className="text-muted-foreground mt-0.5 block truncate text-[11px]">
                              {device.description}
                            </span>
                          </span>
                        </button>
                      </li>
                    )
                  })}
                </ul>
              )}
        </div>
      </div>

      <div className="border-border/60 mt-auto border-t px-5 py-3.5">
        <div className="flex items-center justify-between text-[11px]">
          <span className="text-muted-foreground">自动刷新</span>
          <span
            className={cn(
              'inline-flex items-center gap-1.5 font-medium',
              auto
                ? 'text-emerald-600 dark:text-emerald-400'
                : 'text-muted-foreground',
            )}
          >
            <span
              className={cn(
                'size-1.5 rounded-full',
                auto ? 'bg-emerald-500' : 'bg-muted-foreground/40',
              )}
            />
            {auto ? `${autoIntervalSec}s` : '已暂停'}
          </span>
        </div>
        <p className="text-muted-foreground/70 mt-1.5 text-[10px]">
          本地 HID 读取 · 不上传
        </p>
      </div>
    </aside>
  )
}
