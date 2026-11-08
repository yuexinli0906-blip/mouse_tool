/** 无边框窗口标题栏：拖拽区、GitHub/设置、最小化/最大化/关闭。 */
import { Icon } from '@iconify/react'
import { useNavigate, useRouterState } from '@tanstack/react-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

const GITHUB_URL = 'https://github.com/wjyugutou/mouse_tool'

export function TitleBar() {
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const [maximized, setMaximized] = useState(false)

  async function syncMaximized() {
    try {
      setMaximized(await getCurrentWindow().isMaximized())
    } catch {
      /* browser preview */
    }
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined
    let cancelled = false
    // 初始态与后续 resize 都在 await 回调里 setState，避免 effect 体内同步改状态。
    void (async () => {
      try {
        const win = getCurrentWindow()
        const next = await win.isMaximized()
        if (!cancelled) setMaximized(next)
        unlisten = await win.onResized(() => {
          void syncMaximized()
        })
      } catch {
        /* ignore */
      }
    })()
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  const minimize = () => {
    void getCurrentWindow().minimize()
  }
  const toggleMaximize = () => {
    void getCurrentWindow()
      .toggleMaximize()
      .then(() => syncMaximized())
  }
  const close = () => {
    void getCurrentWindow().close()
  }

  return (
    <header
      className="bg-card/80 flex h-9 shrink-0 items-stretch border-b border-border backdrop-blur"
      data-tauri-drag-region
    >
      <div className="flex items-center gap-0.5 px-2">
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          title="GitHub"
          aria-label="GitHub"
          onClick={() => void openUrl(GITHUB_URL)}
        >
          <Icon icon="mdi:github" className="size-4" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          title="设置"
          aria-label="设置"
          onClick={() => {
            if (pathname !== '/settings') void navigate({ to: '/settings' })
          }}
        >
          <Icon icon="mdi:cog-outline" className="size-4" />
        </Button>
      </div>
      <div className="min-w-6 flex-1" data-tauri-drag-region onDoubleClick={toggleMaximize} />
      <div className="flex items-stretch">
        <button
          type="button"
          className="text-muted-foreground hover:bg-accent hover:text-accent-foreground inline-flex w-11 items-center justify-center"
          title="最小化"
          aria-label="最小化"
          onClick={minimize}
        >
          <Icon icon="mdi:window-minimize" className="size-3.5" />
        </button>
        <button
          type="button"
          className="text-muted-foreground hover:bg-accent hover:text-accent-foreground inline-flex w-11 items-center justify-center"
          title={maximized ? '还原' : '最大化'}
          aria-label={maximized ? '还原' : '最大化'}
          onClick={toggleMaximize}
        >
          <Icon
            icon={maximized ? 'mdi:window-restore' : 'mdi:window-maximize'}
            className="size-3.5"
          />
        </button>
        <button
          type="button"
          className={cn(
            'text-muted-foreground inline-flex w-11 items-center justify-center',
            'hover:bg-destructive hover:text-white',
          )}
          title="关闭"
          aria-label="关闭"
          onClick={close}
        >
          <Icon icon="mdi:close" className="size-3.5" />
        </button>
      </div>
    </header>
  )
}
