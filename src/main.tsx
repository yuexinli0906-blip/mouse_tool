/** 前端入口：先启动持久化设置，再挂载路由。 */
import { createRouter, RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { routeTree } from './routeTree.gen'
import { applyThemeFromStore, settingsTauriStore, useSettingsStore } from './stores/settings'
import './styles/index.css'

const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
})
  
declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

const rootEl = document.getElementById('root')
if (!rootEl) {
  throw new Error('未找到 #root 挂载点')
}

document.documentElement.classList.add('dark')

async function bootstrap(mount: HTMLElement) {
  // 先加载磁盘上的设置，再渲染，避免闪默认值
  try {
    await settingsTauriStore.start()
  } catch (error) {
    console.error('settings store start failed', error)
  }

  applyThemeFromStore(useSettingsStore.getState().themeMode)

  createRoot(mount).render(
    <StrictMode>
      <RouterProvider router={router} />
    </StrictMode>,
  )
}

void bootstrap(rootEl)
