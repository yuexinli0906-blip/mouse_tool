/** 根布局：自定义标题栏 + 主题同步 + 子路由出口。 */
import { Outlet } from '@tanstack/react-router'
import { useEffect } from 'react'
import { TitleBar } from '@/components/TitleBar'
import { applyThemeFromStore, useSettingsStore } from '@/stores/settings'

export function RootLayout() {
  const themeMode = useSettingsStore((s) => s.themeMode)

  useEffect(() => {
    applyThemeFromStore(themeMode)
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => applyThemeFromStore(themeMode)
    mq.addEventListener('change', onChange)
    return () => mq.removeEventListener('change', onChange)
  }, [themeMode])

  return (
    <div className="bg-background text-foreground flex h-full flex-col">
      <TitleBar />
      <Outlet />
    </div>
  )
}
