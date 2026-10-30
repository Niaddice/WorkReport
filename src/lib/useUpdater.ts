import { computed, ref, shallowRef } from 'vue'
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { Update } from '@tauri-apps/plugin-updater'
import { version } from '../../package.json'
import { dismissMessage, pushMessage } from './messages'

export const UPDATE_DOWNLOAD_URL = 'https://github.com/Niaddice/WorkReport/releases/latest'

type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'preparing'
  | 'installing'
  | 'installed'
  | 'restarting'
  | 'latest'
  | 'unpublished'
  | 'error'

function updateFailure(cause: unknown, fallback: string) {
  const detail = cause instanceof Error ? cause.message : String(cause)
  if (/signature|minisign|public key/i.test(detail)) return '更新包校验失败，请稍后重新下载。'
  // Preserve actionable local save errors, keeping transport internals out of the UI.
  return /[\u4e00-\u9fff]/.test(detail) ? `${fallback}${detail}` : fallback
}

export function useUpdater(options: { beforeInstall: () => Promise<boolean> }) {
  const supported = isTauri()
  const status = ref<UpdateStatus>('idle')
  const release = shallowRef<{ version: string; notes: string; date?: string } | null>(null)
  const error = ref('')
  const visible = ref(false)
  const downloaded = ref(0)
  const total = ref<number>()
  const lastChecked = ref('')
  const manualDownloadAvailable = ref(!supported)
  const manualDownloadOpening = ref(false)
  const manualDownloadError = ref('')
  const busy = computed(() => ['downloading', 'preparing', 'installing', 'restarting'].includes(status.value))
  const checking = computed(() => status.value === 'checking')
  const progress = computed(() =>
    total.value ? Math.min(100, Math.floor((downloaded.value / total.value) * 100)) : undefined,
  )
  let update: Update | null = null
  let readyToInstall = false
  let disposed = false
  let messageId = 0
  let fallbackNotified = false

  async function openManualDownload() {
    if (disposed || manualDownloadOpening.value) return
    manualDownloadOpening.value = true
    manualDownloadError.value = ''
    try {
      if (supported) await invoke('open_update_download')
      else window.open(UPDATE_DOWNLOAD_URL, '_blank', 'noopener,noreferrer')
    } catch {
      if (disposed) return
      manualDownloadError.value = '无法打开浏览器，请复制下载地址后手动打开。'
      pushMessage({ tone: 'error', text: `${manualDownloadError.value}${UPDATE_DOWNLOAD_URL}`, duration: 0 })
    } finally {
      manualDownloadOpening.value = false
    }
  }

  function notifyManualDownload(manual: boolean, text: string) {
    if (!manual && fallbackNotified) return
    fallbackNotified = true
    if (messageId) dismissMessage(messageId)
    messageId = pushMessage({
      tone: 'warning',
      title: '可手动下载更新',
      text,
      duration: 0,
      actions: [{ label: '打开下载页面', run: () => void openManualDownload() }],
    })
  }

  function open() {
    if (release.value && !disposed) visible.value = true
  }
  function close() {
    if (!busy.value) visible.value = false
  }
  async function releaseResource(resource: Update | null) {
    try {
      await resource?.close()
    } catch {
      /* Closing a stale IPC resource must not interrupt work. */
    }
  }

  async function checkForUpdates(manual = true) {
    if (disposed || checking.value || busy.value || status.value === 'installed') return
    if (!supported) {
      error.value = '请在桌面 App 中检查更新。'
      manualDownloadAvailable.value = true
      return
    }
    status.value = 'checking'
    error.value = ''
    try {
      const { check } = await import('@tauri-apps/plugin-updater')
      const next = await check({ timeout: 15_000 })
      if (disposed) {
        await releaseResource(next)
        return
      }
      await releaseResource(update)
      if (disposed) {
        await releaseResource(next)
        return
      }
      update = next
      readyToInstall = false
      manualDownloadAvailable.value = false
      manualDownloadError.value = ''
      downloaded.value = 0
      total.value = undefined
      if (messageId) dismissMessage(messageId)
      lastChecked.value = new Date().toISOString()
      release.value = next ? { version: next.version, notes: next.body ?? '', date: next.date } : null
      status.value = next ? 'available' : 'latest'
      if (next) {
        if (manual) open()
        else
          messageId = pushMessage({
            title: '发现新版本',
            text: `WorkReport ${next.version} 已可更新。`,
            duration: 0,
            actions: [{ label: '查看更新', run: open }],
          })
      } else if (manual) pushMessage({ tone: 'success', text: '当前已是最新版本。' })
    } catch (cause) {
      if (disposed) return
      if (!update && /could not fetch a valid release json/i.test(String(cause))) {
        try {
          const published = await invoke<boolean>('has_published_update')
          if (disposed) return
          if (published === false) {
            status.value = 'unpublished'
            error.value = ''
            lastChecked.value = new Date().toISOString()
            manualDownloadAvailable.value = true
            notifyManualDownload(manual, '暂未发布支持自动更新的版本，可打开下载页面手动下载更新。')
            return
          }
        } catch {
          // An unreachable/rate-limited metadata API must remain a check error.
        }
      }
      if (disposed) return
      status.value = update ? 'available' : 'error'
      error.value = updateFailure(cause, '暂时无法获取更新信息，请检查网络或稍后重试。')
      manualDownloadAvailable.value = true
      notifyManualDownload(manual, `${error.value}也可打开下载页面手动下载更新。`)
    }
  }

  async function restart(prepared = false) {
    if (status.value !== 'installed') return
    error.value = ''
    status.value = 'preparing'
    try {
      if (!prepared && !(await options.beforeInstall())) {
        status.value = 'installed'
        error.value = '更新已安装，请先完成当前工作，再重新启动。'
        return
      }
      status.value = 'restarting'
      const { relaunch } = await import('@tauri-apps/plugin-process')
      await relaunch()
    } catch (cause) {
      status.value = 'installed'
      error.value = updateFailure(cause, '更新已安装，请重试重启或手动重新打开 App。')
    }
  }

  async function install() {
    if (!update || disposed || busy.value || checking.value || status.value === 'installed') return
    error.value = ''
    manualDownloadAvailable.value = false
    manualDownloadError.value = ''
    try {
      if (!readyToInstall) {
        status.value = 'downloading'
        downloaded.value = 0
        total.value = undefined
        await update.download(
          (event) => {
            if (event.event === 'Started') total.value = event.data.contentLength
            if (event.event === 'Progress') downloaded.value += event.data.chunkLength
          },
          { timeout: 120_000 },
        )
        readyToInstall = true
      }
      if (disposed) {
        await releaseResource(update)
        return
      }
      // Download first, then flush immediately before install (Windows exits here).
      status.value = 'preparing'
      if (!(await options.beforeInstall())) {
        status.value = 'available'
        error.value = '当前工作尚未保存或仍有任务在运行，请完成后重试。'
        return
      }
      if (disposed) {
        await releaseResource(update)
        return
      }
      status.value = 'installing'
      await update.install()
      status.value = 'installed'
      await releaseResource(update)
      update = null
      await restart(true)
    } catch (cause) {
      // install() consumes the downloaded bytes even if installation fails.
      manualDownloadAvailable.value = status.value === 'downloading' || status.value === 'installing'
      readyToInstall = false
      status.value = 'available'
      error.value = updateFailure(cause, '更新未完成，请检查网络或当前工作后重试。')
    }
  }

  function dispose() {
    disposed = true
    if (messageId) dismissMessage(messageId)
    if (!busy.value) void releaseResource(update)
  }
  return {
    supported,
    currentVersion: version,
    status,
    release,
    error,
    visible,
    downloaded,
    total,
    lastChecked,
    manualDownloadUrl: UPDATE_DOWNLOAD_URL,
    manualDownloadAvailable,
    manualDownloadOpening,
    manualDownloadError,
    busy,
    checking,
    progress,
    checkForUpdates,
    openManualDownload,
    open,
    close,
    install,
    restart,
    dispose,
  }
}

export type AppUpdater = ReturnType<typeof useUpdater>
