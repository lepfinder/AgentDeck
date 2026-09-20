import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { I18nProvider } from './i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauri } from './api/tauriBridge'
import { QuotaPillApp } from './components/quotaPill/QuotaPillApp'

function isQuotaPillWindow(): boolean {
  // 浏览器预览入口：/?view=quota-pill（可加 &mock=1 看假数据效果）
  if (window.location.search.includes('view=quota-pill')) return true
  return isTauri() && getCurrentWindow().label === 'quota-pill'
}

// 透明悬浮条窗口：必须在渲染前清掉全局主题底色，否则圆角外四角会露出灰底
if (isQuotaPillWindow()) {
  document.documentElement.classList.add('quota-pill-window')
  document.documentElement.style.background = 'transparent'
  document.body.style.background = 'transparent'
  document.body.style.backgroundColor = 'transparent'
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <I18nProvider>
      {isQuotaPillWindow() ? <QuotaPillApp /> : <App />}
    </I18nProvider>
  </StrictMode>,
)
