/** 首页装饰用形态：随机表情，停顿后再换下一句几率换形状与配色。 */
import type { MorphBotElement, MorphBotState } from '@/vendor/morph-bot/morph-bot.js'
import { useLayoutEffect, useRef } from 'react'
import { MORPH_BOT_SHAPES, MORPH_BOT_STATES } from '@/vendor/morph-bot/morph-bot.js'

/** 体色：避开可能看不清眼睛的实心深色。 */
const BODY_COLORS = [
  '#705cff',
  '#3c82f6',
  '#f9705c',
  '#27b98b',
  '#e06ca8',
  '#f08a3e',
  '#38bdf8',
  '#a78bfa',
  '#2dd4bf',
  '#fb7185',
] as const

/** 单次表情最短 / 最长停顿（毫秒）；实际随机落在区间内。 */
const HOLD_MIN_MS = 1500
const HOLD_MAX_MS = 4000

/** 自主扫视幅度；指针跟随时引擎会压低其权重，但仍略增强空闲视线。 */
const GAZE_SCALE = 2.25

function randomHold(): number {
  return HOLD_MIN_MS + Math.random() * (HOLD_MAX_MS - HOLD_MIN_MS)
}

/** 从列表挑一个尽量不同于 current 的项；列表只剩一项时退回该项。 */
function pickOther<T extends string>(items: readonly T[], current: string): T {
  const pool = items.filter((item) => item !== current)
  const source = pool.length > 0 ? pool : items
  const index = Math.floor(Math.random() * source.length)
  return source[index] ?? (source[0] as T)
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms)
  })
}

function pointInHost(host: HTMLElement, clientX: number, clientY: number, pad = 6): boolean {
  const r = host.getBoundingClientRect()
  return (
    clientX >= r.left - pad &&
    clientX <= r.right + pad &&
    clientY >= r.top - pad &&
    clientY <= r.bottom + pad
  )
}

interface Props {
  /** CSS 像素；默认塞进顶栏空隙。 */
  size?: number
}

export function MorphBot({ size = 160 }: Props) {
  const hostRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const host = hostRef.current
    if (!host) return

    const bot: MorphBotElement = document.createElement('morph-bot')
    bot.setAttribute('decorative', '')
    bot.toggleAttribute('follow-pointer', true)
    bot.size = size
    bot.shape = pickOther(MORPH_BOT_SHAPES, '')
    bot.setAttribute('eye-color', '#f4f7fb')
    bot.setMaterial('solid', { color: pickOther(BODY_COLORS, '') })
    bot.configure({
      states: Object.fromEntries(
        MORPH_BOT_STATES.map((state) => [state, { gazeScale: GAZE_SCALE }]),
      ) as Record<MorphBotState, { gazeScale: number }>,
    })
    host.replaceChildren(bot)

    // effect 清理时置 true，避免卸载后异步回调仍改 DOM/切形态
    let cancelled = false
    /**
     * 悬停检测不用 host 的 pointerenter：morph-bot 是 open shadow + SVG，
     * 在部分 WebView（含 Tauri）里宿主 enter/leave 不可靠，导致上一版暂停无效。
     * 改为 window pointermove + 包围盒，与引擎全局跟手同一路径。
     */
    let hoverPaused = false

    const setHoverPaused = (next: boolean) => {
      if (hoverPaused === next) return
      hoverPaused = next
      if (next) bot.stopSequence()
    }

    const syncHoverFromEvent = (event: PointerEvent) => {
      if (cancelled) return
      setHoverPaused(pointInHost(host, event.clientX, event.clientY))
    }

    const waitWhileHoverPaused = async () => {
      while (true) {
        if (!hoverPaused || cancelled) break
        await sleep(80)
      }
    }

    const onWindowPointerLeave = () => setHoverPaused(false)
    const onDocMouseLeave = () => setHoverPaused(false)
    window.addEventListener('pointermove', syncHoverFromEvent, { passive: true })
    // 指针离开窗口时也要恢复自动切换
    window.addEventListener('blur', onWindowPointerLeave)
    document.documentElement.addEventListener('mouseleave', onDocMouseLeave)

    const run = async () => {
      let state: MorphBotState = pickOther(MORPH_BOT_STATES, '')
      while (true) {
        if (cancelled) break
        await waitWhileHoverPaused()
        if (cancelled) return

        let result: { cancelled: boolean }
        try {
          result = await bot.playSequence([{ state, hold: randomHold() }], { loop: false })
        } catch {
          return
        }
        if (cancelled) return

        // 悬停打断（stopSequence）→ 等到离开再开下一轮，切勿把循环结束掉
        if (result.cancelled || hoverPaused) {
          await waitWhileHoverPaused()
          continue
        }

        await waitWhileHoverPaused()
        if (cancelled || hoverPaused) continue

        if (Math.random() < 0.5) {
          bot.setShape(pickOther(MORPH_BOT_SHAPES, bot.shape))
          bot.setMaterial('solid', {
            color: pickOther(BODY_COLORS, bot.getAttribute('color') ?? ''),
          })
        }
        state = pickOther(MORPH_BOT_STATES, bot.state)
      }
    }

    void run()

    return () => {
      cancelled = true
      hoverPaused = false
      window.removeEventListener('pointermove', syncHoverFromEvent)
      window.removeEventListener('blur', onWindowPointerLeave)
      document.documentElement.removeEventListener('mouseleave', onDocMouseLeave)
      bot.stopSequence()
      host.replaceChildren()
    }
  }, [size])

  return (
    <div
      ref={hostRef}
      className="inline-flex shrink-0 items-center justify-center"
      style={{ width: size, height: size, pointerEvents: 'auto', position: 'relative' }}
    />
  )
}
