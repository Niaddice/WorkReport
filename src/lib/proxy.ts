import { invoke } from '@tauri-apps/api/core'

export interface ProxySettings {
  mode: 'system' | 'direct' | 'manual'
  url: string
  noProxy: string
}
export const DEFAULT_PROXY_SETTINGS: ProxySettings = {
  mode: 'system',
  url: '',
  noProxy: 'localhost,127.0.0.1,::1',
}
export function cloneProxySettings(value: ProxySettings = DEFAULT_PROXY_SETTINGS): ProxySettings {
  return { mode: value.mode, url: value.url, noProxy: value.noProxy }
}
export function validateProxySettings(settings: ProxySettings): string {
  if (
    settings.url.length > 2048 ||
    settings.noProxy.length > 4096 ||
    /[\u0000-\u001f\u007f]/.test(settings.url + settings.noProxy)
  )
    return '代理地址或绕过列表过长，或包含无效字符。'
  if (settings.mode !== 'manual') return ''
  try {
    const url = new URL(settings.url.trim())
    if (!['http:', 'https:', 'socks5:', 'socks5h:'].includes(url.protocol) || !url.hostname || url.port === '0')
      return '请填写有效的 HTTP、HTTPS、SOCKS5 或 SOCKS5H 代理地址。'
    if (url.username || url.password || !['', '/'].includes(url.pathname) || url.search || url.hash)
      return '代理地址请勿包含账号密码、路径或查询参数。'
    if (url.protocol.startsWith('socks5') && !url.port) return 'SOCKS5 代理地址需要填写端口。'
  } catch {
    return '请填写完整的代理地址，例如 http://127.0.0.1:7890。'
  }
  return ''
}
export const proxyApi = {
  load: () => invoke<ProxySettings | undefined>('load_proxy_settings').then(cloneProxySettings),
  save: (settings: ProxySettings) =>
    invoke<ProxySettings>('save_proxy_settings', { settings: cloneProxySettings(settings) }),
  test: (settings: ProxySettings) => invoke<string>('test_proxy_settings', { settings: cloneProxySettings(settings) }),
}
