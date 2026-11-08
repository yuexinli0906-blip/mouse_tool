/** 电量 hero 卡：大号电量环、状态、刷新操作、底部统计与可折叠诊断。 */
import type { BatteryInfo } from '@/devices'
import { Icon } from '@iconify/react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

type BatteryLevel = 'high' | 'mid' | 'low' | 'unknown' | 'empty' | 'charging' | 'unsupported'

interface Props {
  deviceName: string
  info: BatteryInfo | null
  loading: boolean
  auto: boolean
  autoIntervalSec: number
  error: string | null
  lastUpdated: Date | null
  onRefresh: () => void
  onStartRefresh: () => void
  onStopRefresh: () => void
}

const LEVEL_LABEL: Record<BatteryLevel, string> = {
  high: '电量充足',
  mid: '电量正常',
  low: '电量偏低',
  unknown: '等待数据',
  empty: '未连接',
  charging: '充电中',
  unsupported: '型号不支持',
}

const LEVEL_STROKE: Record<BatteryLevel, string> = {
  high: 'text-emerald-500 dark:text-emerald-400',
  mid: 'text-amber-500 dark:text-amber-400',
  low: 'text-red-500 dark:text-red-400',
  unknown: 'text-muted-foreground/35',
  empty: 'text-border',
  charging: 'text-sky-500 dark:text-sky-400',
  unsupported: 'text-muted-foreground/60',
}

const LEVEL_GLOW: Record<BatteryLevel, string> = {
  high: 'bg-emerald-500/10',
  mid: 'bg-amber-500/10',
  low: 'bg-red-500/10',
  unknown: 'bg-primary/5',
  empty: 'bg-transparent',
  charging: 'bg-sky-500/10',
  unsupported: 'bg-muted/40',
}

function levelFromInfo(info: BatteryInfo | null): BatteryLevel {
  if (!info)
    return 'unknown'
  // 型号不支持：设备插着、但本版读不了，显示「未连接」会让人以为没插。
  if (info.unsupported)
    return 'unsupported'
  if (!info.connected)
    return 'empty'
  // 充电中必须先判：部分设备充电时拿不到百分比（如前行者 S9 Pro 的 0xE4 哨兵值），
  // 若先判 percent == null 会落到「等待数据」，用户会误以为读不到电量。
  if (info.charging)
    return 'charging'
  if (info.percent == null)
    return 'unknown'
  if (info.percent <= 15)
    return 'low'
  if (info.percent <= 40)
    return 'mid'
  return 'high'
}

function percentText(info: BatteryInfo | null): string {
  if (!info || !info.connected || info.percent == null)
    return '--'
  return `${Math.max(0, Math.min(100, Math.round(info.percent)))}%`
}

function statusText(
  info: BatteryInfo | null,
  loading: boolean,
  error: string | null,
): string {
  if (error)
    return '读取失败'
  if (!info && !error)
    return loading ? '正在读取…' : '等待读取'
  if (!info || !info.connected)
    return '设备未连接'
  // 型号不受支持：必须直说，不能落到下面「等待电量数据」，
  // 否则用户会去查鼠标开机/重插接收器，而真正原因是这只设备本版不支持。
  if (info.unsupported)
    return '该型号暂不支持'
  // 充电中要说「充电中」，不能因为拿不到百分比就说「等待电量数据」。
  if (info.charging)
    return '充电中'
  if (info.percent == null)
    return '等待电量数据'
  if (info.percent <= 15)
    return '电量偏低'
  return '运行正常'
}

function hintText(
  info: BatteryInfo | null,
  loading: boolean,
  error: string | null,
  auto: boolean,
): string {
  if (error)
    return '请检查设备连接后重试。'
  if (!info && !error)
    return loading ? '正在从设备读取 HID 数据。' : '准备读取设备状态。'
  if (!info || !info.connected)
    return '未检测到接收器或设备，请检查连接后重试。'
  // 型号不支持时，原因写在 detail 里（后端给出具体型号），直接显示给用户看。
  if (info.unsupported) {
    return info.detail || '检测到的设备型号不在本版支持范围内。'
  }
  // 充电中优先说明「正在充电」，而不是让人以为读不到数据。
  if (info.charging) {
    return info.percent == null
      ? '正在充电。该设备充电时不上报百分比，充满或拔线后会自动更新。'
      : '正在充电，充满后会自动更新。'
  }
  if (info.percent == null)
    return '接收器已连接，等待鼠标唤醒。动一下鼠标或重新插拔接收器。'
  if (info.percent <= 15)
    return '建议尽快充电。'
  return auto ? '自动刷新已开启，电量会按间隔更新。' : '自动刷新已暂停，可随时手动刷新。'
}

/**
 * 型号认不出来时，主界面（标题旁徽章）要显示的说明。
 *
 * 2.4G 接收器 PID 被同系列多型号共用时，标题的 manifest 名会把型号说死成
 * 「前行者 S9 Pro」，而实际可能是 Ultra / Max。这条说明必须出现在主界面，
 * 不能只放折叠的「诊断详情」（review 意见 #3）。
 * 返回 null 表示不需要显示。
 */
function modelUncertainText(info: BatteryInfo | null): string | null {
  if (!info || !info.connected || !info.model_uncertain)
    return null
  return '型号按 S9 系列处理'
}

function formatClock(value: Date | null): string {
  if (!value)
    return '—'
  return value.toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

function truncate(value: string, max = 120): string {
  return value.length > max ? `${value.slice(0, max)}…` : value
}

function BatteryGauge({
  level,
  text,
  charging,
  loading,
}: {
  level: BatteryLevel
  text: string
  charging: boolean
  loading: boolean
}) {
  const size = 208
  const stroke = 12
  const radius = (size - stroke) / 2 - 6
  const circumference = 2 * Math.PI * radius
  const percent = text.endsWith('%') ? Number.parseInt(text, 10) : null
  const ratio = percent == null || Number.isNaN(percent) ? 0 : percent / 100
  const fill = level !== 'empty'

  return (
    <div className="relative mx-auto flex size-[208px] items-center justify-center">
      <div className={cn('absolute inset-8 rounded-full blur-2xl', LEVEL_GLOW[level])} />
      <svg
        viewBox={`0 0 ${size} ${size}`}
        className="relative size-full -rotate-90"
        aria-hidden
      >
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeWidth={stroke}
          className="text-border/60"
        />
        {fill && (
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke="currentColor"
            strokeWidth={stroke}
            strokeLinecap="round"
            className={cn(
              LEVEL_STROKE[level],
              'transition-[stroke-dashoffset,stroke] duration-700 ease-out',
            )}
            strokeDasharray={level === 'unknown' ? '3 10' : circumference}
            strokeDashoffset={level === 'unknown' ? 0 : circumference * (1 - ratio)}
          />
        )}
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span
          className={cn(
            'text-[56px] leading-none font-semibold tracking-tight tabular-nums',
            loading && text === '--' && 'animate-pulse',
          )}
        >
          {text}
        </span>
        <span className="text-muted-foreground mt-2 text-[10px] font-medium tracking-[0.18em] uppercase">
          {charging ? '充电中' : LEVEL_LABEL[level]}
        </span>
      </div>
      {charging && (
        <span className="border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400 absolute top-2 right-2 flex size-8 items-center justify-center rounded-full border">
          <Icon icon="mdi:flash" className="size-4" />
        </span>
      )}
    </div>
  )
}

function Stat({ label, value }: { label: string, value: string }) {
  return (
    <div className="min-w-0 px-4 py-3">
      <p className="text-muted-foreground text-[10px] font-medium tracking-[0.16em] uppercase">
        {label}
      </p>
      <p className="mt-1 truncate text-sm font-medium">{value}</p>
    </div>
  )
}

export function BatteryCard({
  deviceName,
  info,
  loading,
  auto,
  autoIntervalSec,
  error,
  lastUpdated,
  onRefresh,
  onStartRefresh,
  onStopRefresh,
}: Props) {
  const level = levelFromInfo(info)
  const connected = Boolean(info?.connected)
  const charging = Boolean(info?.charging)
  const connectionText = !info && !error
    ? '检测中'
    : connected
      ? '已连接'
      : '未连接'
  const chargingText = info?.charging == null
    ? '—'
    : info.charging
      ? '充电中'
      : '未充电'

  return (
    <section className="border-border/70 bg-card/80 relative w-full overflow-hidden rounded-[28px] border shadow-[0_24px_80px_-48px_rgba(0,0,0,0.55)] backdrop-blur-sm">
      <div className="bg-primary/10 pointer-events-none absolute -top-28 -right-24 size-72 rounded-full blur-3xl" />
      <div className="relative p-6 lg:p-8">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <p className="text-muted-foreground text-[10px] font-medium tracking-[0.2em] uppercase">
              当前设备
            </p>
            <h1 className="mt-1.5 truncate text-2xl font-semibold tracking-tight">
              {deviceName}
            </h1>
            <div className="mt-2.5 flex flex-wrap items-center gap-2">
              <span
                className={cn(
                  'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[11px] font-medium',
                  connected
                    ? 'border-emerald-500/25 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                    : 'border-border/60 bg-muted/40 text-muted-foreground',
                )}
              >
                <span
                  className={cn(
                    'size-1.5 rounded-full',
                    connected ? 'bg-emerald-500' : 'bg-muted-foreground/50',
                  )}
                />
                {connectionText}
              </span>
              {charging && (
                <span className="border-amber-500/25 bg-amber-500/10 text-amber-600 dark:text-amber-400 inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-[11px] font-medium">
                  <Icon icon="mdi:flash" className="size-3" />
                  充电中
                </span>
              )}
              {/* 型号认不出来时必须写在主界面（标题旁），不能只放折叠的诊断详情：
                  2.4G 接收器 PID 被同系列多型号共用时，标题的 manifest 名会把型号说死。 */}
              {modelUncertainText(info) && (
                <span
                  className="border-border/60 bg-muted/40 text-muted-foreground inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-[11px] font-medium"
                  title="2.4G 接收器 PID 由同系列多型号共用，无法从 HID 区分具体型号"
                >
                  <Icon icon="mdi:help-circle-outline" className="size-3" />
                  {modelUncertainText(info)}
                </span>
              )}
            </div>
          </div>

          <span className="border-border/60 bg-background/40 text-muted-foreground inline-flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-[11px]">
            <span
              className={cn(
                'size-1.5 rounded-full',
                auto ? 'bg-emerald-500' : 'bg-muted-foreground/50',
              )}
            />
            {auto ? `自动刷新 ${autoIntervalSec}s` : '自动刷新已暂停'}
          </span>
        </div>

        <div className="mt-7 grid items-center gap-7 sm:grid-cols-[208px_minmax(0,1fr)]">
          <BatteryGauge
            level={level}
            text={percentText(info)}
            charging={charging}
            loading={loading}
          />

          <div className="min-w-0 text-center sm:text-left">
            <p className="text-lg font-medium">{statusText(info, loading, error)}</p>
            <p className="text-muted-foreground mt-1.5 text-sm leading-relaxed">
              {hintText(info, loading, error, auto)}
            </p>
            <div className="mt-6 flex flex-wrap justify-center gap-2 sm:justify-start">
              <Button
                type="button"
                size="sm"
                onClick={onRefresh}
                disabled={loading}
                className="rounded-full px-4"
              >
                <Icon
                  icon="mdi:refresh"
                  className={cn('size-4', loading && 'animate-spin')}
                />
                {loading ? '读取中…' : '立即刷新'}
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={auto ? onStopRefresh : onStartRefresh}
                className="rounded-full px-4"
              >
                <Icon icon={auto ? 'mdi:pause' : 'mdi:play'} className="size-4" />
                {auto ? '暂停自动刷新' : '开始自动刷新'}
              </Button>
            </div>
          </div>
        </div>

        {error && (
          <div className="border-destructive/25 bg-destructive/10 text-destructive mt-6 flex items-start gap-2 rounded-xl border px-3 py-2 text-xs">
            <Icon icon="mdi:alert-circle-outline" className="mt-0.5 size-4 shrink-0" />
            <span className="min-w-0 break-all">{`读取失败：${truncate(error)}`}</span>
          </div>
        )}

        <div className="border-border/60 bg-background/40 mt-6 grid grid-cols-3 divide-x divide-border/60 overflow-hidden rounded-2xl border">
          <Stat label="连接" value={connectionText} />
          <Stat label="充电" value={chargingText} />
          <Stat label="最后刷新" value={formatClock(lastUpdated)} />
        </div>

      </div>
    </section>
  )
}

export function BatteryDiagnostics({ info, error }: { info: BatteryInfo | null, error: string | null }) {
  return (
    <details className="group border-border/60 bg-background/30 mt-4 rounded-xl border px-4 py-3">
      <summary className="text-muted-foreground flex cursor-pointer list-none items-center justify-between text-xs select-none [&::-webkit-details-marker]:hidden">
        诊断详情
        <Icon
          icon="mdi:chevron-down"
          className="size-4 transition-transform group-open:rotate-180"
        />
      </summary>
      <div className="text-muted-foreground mt-3 space-y-2 text-[11px] leading-relaxed">
        <p className="break-all">{info?.device || '—'}</p>
        <p className="whitespace-pre-wrap break-all">{info?.detail || error || '—'}</p>
      </div>
    </details>
  )
}
