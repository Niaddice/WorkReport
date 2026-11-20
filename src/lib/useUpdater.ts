import { computed, ref, shallowRef } from 'vue'
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { Update } from '@tauri-apps/plugin-updater'
import { version } from '../../package.json'
import { dismissMessage, pushMessage } from './messages'

export const UPDATE_DOWNLOAD_URL = 'https://github.com/Niaddice/WorkReport/releases/latest'
export const DEV_DOWNLOAD_URL = 'https://github.com/Niaddice/WorkReport/releases'
export const UPDATE_CHANNEL_KEY = 'work-report.update-channel'
const CHANNEL_SWITCH_KEY = 'work-report.update-channel-switch'
export type UpdateChannel = 'main' | 'dev'
export const UPDATE_CHANNEL_OPTIONS = [
  { value: 'main', label: '正式版（main）' },
  { value: 'dev', label: '开发版（dev）' },
]

function readPreference(key: string) {
  try {
    return globalThis.localStorage.getItem(key)
  } catch {
    return null
  }
}
function writePreference(key: string, value: string) {
  try {
    globalThis.localStorage.setItem(key, value)
  } catch {
    /* The selected channel still works for this session if storage is unavailable. */
  }
}
interface ChannelCheck {
  update: ConstructorParameters<typeof Update>[0] | null
  published: boolean
  downloadUrl: string
}

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
  const channel = ref<UpdateChannel>(readPreference(UPDATE_CHANNEL_KEY) === 'dev' ? 'dev' : 'main')
  const channelLabel = computed(() => (channel.value === 'dev' ? '开发版（dev）' : '正式版（main）'))
  const resolvedDownloadUrl = ref('')
  const manualDownloadUrl = computed(
    () => resolvedDownloadUrl.value || (channel.value === 'dev' ? DEV_DOWNLOAD_URL : UPDATE_DOWNLOAD_URL),
  )
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
  let allowChannelSwitch = readPreference(CHANNEL_SWITCH_KEY) === channel.value

  async function openManualDownload() {
    if (disposed || manualDownloadOpening.value) return
    manualDownloadOpening.value = true
    manualDownloadError.value = ''
    try {
      if (supported) await invoke('open_update_download', { channel: channel.value })
      else window.open(manualDownloadUrl.value, '_blank', 'noopener,noreferrer')
    } catch {
      if (disposed) return
      manualDownloadError.value = '无法打开浏览器，请复制下载地址后手动打开。'
      pushMessage({ tone: 'error', text: `${manualDownloadError.value}${manualDownloadUrl.value}`, duration: 0 })
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

  async function setChannel(value: string) {
    if (
      !['main', 'dev'].includes(value) ||
      value === channel.value ||
      disposed ||
      checking.value ||
      busy.value ||
      status.value === 'installed'
    )
      return
    channel.value = value as UpdateChannel
    writePreference(UPDATE_CHANNEL_KEY, value)
    // Keep an unfinished channel switch across restarts, including a return
    // from a newer dev build to the published main version.
    allowChannelSwitch = true
    writePreference(CHANNEL_SWITCH_KEY, value)
    const previous = update
    update = null
    readyToInstall = false
    release.value = null
    visible.value = false
    status.value = 'idle'
    error.value = ''
    resolvedDownloadUrl.value = ''
    lastChecked.value = ''
    downloaded.value = 0
    total.value = undefined
    manualDownloadAvailable.value = !supported
    manualDownloadError.value = ''
    fallbackNotified = false
    if (messageId) dismissMessage(messageId)
    await releaseResource(previous)
    if (supported) await checkForUpdates()
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
      const { Update } = await import('@tauri-apps/plugin-updater')
      const result = await invoke<ChannelCheck>('check_channel_update', { channel: channel.value, allowChannelSwitch })
      const next = result.update ? new Update(result.update) : null
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
      resolvedDownloadUrl.value = result.downloadUrl
      readyToInstall = false
      manualDownloadAvailable.value = !result.published
      manualDownloadError.value = ''
      downloaded.value = 0
      total.value = undefined
      if (messageId) dismissMessage(messageId)
      lastChecked.value = new Date().toISOString()
      release.value = next ? { version: next.version, notes: next.body ?? '', date: next.date } : null
      status.value = !result.published ? 'unpublished' : next ? 'available' : 'latest'
      if (!result.published) {
        notifyManualDownload(manual, `${channelLabel.value}暂未发布支持自动更新的版本，可打开下载页面查看。`)
        return
      }
      if (!next) {
        allowChannelSwitch = false
        writePreference(CHANNEL_SWITCH_KEY, '')
      }
      if (next) {
        if (manual) open()
        else
          messageId = pushMessage({
            title: '发现新版本',
            text: `${channelLabel.value} WorkReport ${next.version} 已可更新。`,
            duration: 0,
            actions: [{ label: '查看更新', run: open }],
          })
      } else if (manual) pushMessage({ tone: 'success', text: `当前已是${channelLabel.value}最新版本。` })
    } catch (cause) {
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
      allowChannelSwitch = false
      writePreference(CHANNEL_SWITCH_KEY, '')
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
    channel,
    channelLabel,
    setChannel,
    status,
    release,
    error,
    visible,
    downloaded,
    total,
    lastChecked,
    manualDownloadUrl,
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
