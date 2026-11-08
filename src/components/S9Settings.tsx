import { useCallback, useEffect, useRef, useState } from 'react'
import { commands } from '@/bindings'
import { cn } from '@/lib/utils'

interface S9SettingsData {
  dpiStages: number[]
  activeStage: number
  pollingRate: number
  supportedPollingRates: number[]
}

type SettingsChange
  = | { activeStage: number, pollingRate: null }
    | { activeStage: null, pollingRate: number }

function isSettings(value: unknown): value is S9SettingsData {
  if (!value || typeof value !== 'object')
    return false
  const data = value as Partial<S9SettingsData>
  const positiveInteger = (number: unknown) => typeof number === 'number' && Number.isSafeInteger(number) && number > 0
  return Array.isArray(data.dpiStages)
    && data.dpiStages.length > 0
    && data.dpiStages.every(positiveInteger)
    && typeof data.activeStage === 'number'
    && Number.isInteger(data.activeStage)
    && data.activeStage >= 0
    && data.activeStage < data.dpiStages.length
    && positiveInteger(data.pollingRate)
    && Array.isArray(data.supportedPollingRates)
    && data.supportedPollingRates.length > 0
    && data.supportedPollingRates.every(positiveInteger)
    && data.supportedPollingRates.includes(data.pollingRate!)
}

const controlClass = 'min-h-11 rounded-xl border px-4 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50'

export function S9Settings() {
  const [settings, setSettings] = useState<S9SettingsData | null>(null)
  const [busy, setBusy] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState('正在读取设置…')
  const mountedRef = useRef(false)
  const inFlightRef = useRef(false)
  const generationRef = useRef(0)

  const request = useCallback(async (change?: SettingsChange) => {
    if (!mountedRef.current || inFlightRef.current)
      return
    inFlightRef.current = true
    const current = ++generationRef.current
    setBusy(true)
    setError(null)
    setStatus(change ? '正在写入并读回核对…' : '正在读取设置…')
    try {
      const result = change
        ? await commands.setS9Settings(change.activeStage, change.pollingRate)
        : await commands.getS9Settings()
      if (!mountedRef.current || current !== generationRef.current)
        return
      if (!isSettings(result))
        throw new Error('设备返回的设置不完整或无效')
      if (change && ((change.activeStage !== null && result.activeStage !== change.activeStage)
        || (change.pollingRate !== null && result.pollingRate !== change.pollingRate))) {
        throw new Error('读回结果与请求不一致')
      }
      setSettings(result)
      setStatus(change ? '设置已核对' : '当前设置已核对')
    }
    catch (e: unknown) {
      if (!mountedRef.current || current !== generationRef.current)
        return
      // 写入可能已经发生，失败后不再展示旧值，也不允许继续写入。
      setSettings(null)
      setError(String(e))
      setStatus('设置未核对，请重新读取')
    }
    finally {
      if (mountedRef.current && current === generationRef.current) {
        inFlightRef.current = false
        setBusy(false)
      }
    }
  }, [])

  useEffect(() => {
    mountedRef.current = true
    const timer = window.setTimeout(() => void request(), 0)
    return () => {
      window.clearTimeout(timer)
      // invoke 无法撤销硬件请求；卸载后仅丢弃结果，防止污染下次挂载。
      mountedRef.current = false
      generationRef.current += 1
      inFlightRef.current = false
    }
  }, [request])

  return (
    <section aria-label="S9 Pro 性能设置" aria-busy={busy} className="bg-card border-border mt-6 rounded-2xl border p-5 sm:p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-foreground text-lg font-semibold">性能设置</h2>
          <p role="status" className="text-muted-foreground mt-1 text-sm">{status}</p>
        </div>
        <button
          type="button"
          disabled={busy}
          onClick={() => void request()}
          className={cn(controlClass, 'border-border bg-background text-foreground hover:bg-muted')}
        >
          {busy ? '处理中…' : '读取设置'}
        </button>
      </div>

      {error && (
        <div role="alert" className="text-destructive mt-4 text-sm">
          <p className="break-words">{error}</p>
          <p className="mt-1">旧设置已清空。请点击「读取设置」，成功后再调整。</p>
        </div>
      )}

      {settings && (
        <div className="mt-6 space-y-6">
          <fieldset disabled={busy}>
            <legend className="text-foreground text-sm font-medium">DPI 档位</legend>
            <p className="text-muted-foreground mt-1 text-xs">选择设备已有档位，DPI 数值仅供查看。</p>
            <div className="mt-3 flex flex-wrap gap-2">
              {settings.dpiStages.map((dpi, index) => (
                <button
                  // 档位索引是协议中的固定身份，DPI 数值可能重复。
                  // eslint-disable-next-line react/no-array-index-key
                  key={index}
                  type="button"
                  aria-pressed={settings.activeStage === index}
                  disabled={busy || settings.activeStage === index}
                  onClick={() => void request({ activeStage: index, pollingRate: null })}
                  className={cn(controlClass, 'min-w-24 tabular-nums', settings.activeStage === index
                    ? 'border-primary bg-primary/10 text-primary disabled:opacity-100'
                    : 'border-border text-foreground hover:bg-muted')}
                >
                  <span className="block text-xs">
                    档位
                    {index + 1}
                    {settings.activeStage === index ? ' · 当前' : ''}
                  </span>
                  <span className="mt-1 block">
                    {dpi}
                    {' '}
                    DPI
                  </span>
                </button>
              ))}
            </div>
          </fieldset>

          <fieldset disabled={busy}>
            <legend className="text-foreground text-sm font-medium">回报率</legend>
            <div className="mt-3 flex flex-wrap gap-2">
              {settings.supportedPollingRates.map(rate => (
                <button
                  key={rate}
                  type="button"
                  aria-pressed={settings.pollingRate === rate}
                  disabled={busy || settings.pollingRate === rate}
                  onClick={() => void request({ activeStage: null, pollingRate: rate })}
                  className={cn(controlClass, 'tabular-nums', settings.pollingRate === rate
                    ? 'border-primary bg-primary/10 text-primary disabled:opacity-100'
                    : 'border-border text-foreground hover:bg-muted')}
                >
                  {rate}
                  {' '}
                  Hz
                  {settings.pollingRate === rate ? ' · 当前' : ''}
                </button>
              ))}
            </div>
          </fieldset>
        </div>
      )}
    </section>
  )
}
