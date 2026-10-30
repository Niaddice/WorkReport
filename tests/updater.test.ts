// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import type { DownloadEvent, Update } from '@tauri-apps/plugin-updater'
import { UPDATE_DOWNLOAD_URL, useUpdater } from '../src/lib/useUpdater'
import UpdateSettings from '../src/components/UpdateSettings.vue'
import { clearMessages, messages } from '../src/lib/messages'

const native = vi.hoisted(() => ({ enabled: true, check: vi.fn(), relaunch: vi.fn(), invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => native.enabled, invoke: native.invoke }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: native.check }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: native.relaunch }))

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}
function fixture() {
  const events: string[] = []
  const update = {
    version: '0.9.0',
    body: '修复保存\n支持更新',
    date: '2026-10-29T12:00:00Z',
    download: vi.fn(async (onEvent: (event: DownloadEvent) => void) => {
      events.push('download')
      onEvent({ event: 'Started', data: { contentLength: 100 } })
      onEvent({ event: 'Progress', data: { chunkLength: 40 } })
      onEvent({ event: 'Progress', data: { chunkLength: 60 } })
      onEvent({ event: 'Finished' })
    }),
    install: vi.fn(async () => {
      events.push('install')
    }),
    close: vi.fn().mockResolvedValue(undefined),
  }
  const save = vi.fn(async () => {
    events.push('save')
    return true
  })
  native.check.mockResolvedValue(update as unknown as Update)
  native.relaunch.mockImplementation(async () => {
    events.push('restart')
  })
  return { updater: useUpdater({ beforeInstall: save }), update, save, events }
}

beforeEach(() => {
  vi.resetAllMocks()
  native.enabled = true
})
afterEach(clearMessages)

describe('desktop updates', () => {
  it('never checks or downloads from the browser preview', async () => {
    native.enabled = false
    const { updater, update } = fixture()
    await updater.checkForUpdates()
    await updater.install()
    expect(updater.supported).toBe(false)
    expect(native.check).not.toHaveBeenCalled()
    expect(update.download).not.toHaveBeenCalled()
  })

  it('announces background updates without opening a modal or downloading', async () => {
    const { updater, update } = fixture()
    await updater.checkForUpdates(false)
    expect(updater.visible.value).toBe(false)
    expect(updater.release.value?.notes).toContain('修复保存')
    expect(update.download).not.toHaveBeenCalled()
    expect(messages.value).toHaveLength(1)
    messages.value[0]!.actions[0]!.run()
    expect(updater.visible.value).toBe(true)
    updater.dispose()
  })

  it('offers a manual download after background failure and clears it after a successful retry', async () => {
    const { updater } = fixture()
    native.check.mockRejectedValueOnce(new Error('offline'))
    await updater.checkForUpdates(false)
    expect(messages.value).toHaveLength(1)
    expect(messages.value[0]?.actions[0]?.label).toBe('打开下载页面')
    expect(updater.manualDownloadAvailable.value).toBe(true)
    expect(updater.error.value).toContain('请检查网络')
    expect(updater.error.value).not.toContain('offline')
    await updater.checkForUpdates()
    expect(updater.visible.value).toBe(true)
    expect(updater.error.value).toBe('')
    expect(updater.manualDownloadAvailable.value).toBe(false)
  })

  it('reports a legacy release with no manifest as unpublished instead of an error or up to date', async () => {
    const { updater } = fixture()
    native.check.mockRejectedValueOnce('Could not fetch a valid release JSON from the remote')
    native.invoke.mockResolvedValueOnce(false)
    await updater.checkForUpdates()
    expect(native.invoke).toHaveBeenCalledWith('has_published_update')
    expect(updater.status.value).toBe('unpublished')
    expect(updater.error.value).toBe('')
    expect(updater.lastChecked.value).not.toBe('')
    expect(messages.value[0]?.text).toContain('暂未发布支持自动更新的版本')
    expect(messages.value[0]?.tone).toBe('warning')
    expect(updater.manualDownloadAvailable.value).toBe(true)
    expect(native.invoke).not.toHaveBeenCalledWith('open_update_download')
  })

  it('reminds once per session when background checks have no update manifest', async () => {
    const { updater } = fixture()
    native.check.mockRejectedValueOnce('Could not fetch a valid release JSON from the remote')
    native.invoke.mockResolvedValueOnce(false)
    await updater.checkForUpdates(false)
    expect(updater.status.value).toBe('unpublished')
    expect(messages.value).toHaveLength(1)
    clearMessages()
    native.check.mockRejectedValueOnce('Could not fetch a valid release JSON from the remote')
    native.invoke.mockResolvedValueOnce(false)
    await updater.checkForUpdates(false)
    expect(messages.value).toHaveLength(0)
    await updater.checkForUpdates()
    expect(updater.status.value).toBe('available')
  })

  it.each(['present', 'unreachable'] as const)(
    'does not hide fetch failures when release metadata is %s',
    async (condition) => {
      const { updater } = fixture()
      native.check.mockRejectedValueOnce('Could not fetch a valid release JSON from the remote')
      if (condition === 'present') native.invoke.mockResolvedValueOnce(true)
      else native.invoke.mockRejectedValueOnce(new Error('rate limited'))
      await updater.checkForUpdates()
      expect(updater.status.value).toBe('error')
      expect(updater.error.value).toContain('请检查网络')
      expect(updater.error.value).not.toContain('JSON')
      expect(messages.value[0]?.tone).toBe('warning')
    },
  )

  it('saves only after a successful download, before installing and restarting', async () => {
    const { updater, update, events } = fixture()
    await updater.checkForUpdates()
    await updater.install()
    expect(events).toEqual(['download', 'save', 'install', 'restart'])
    expect(updater.progress.value).toBe(100)
    expect(update.close).toHaveBeenCalledOnce()
  })

  it('never saves, installs or restarts after download/signature verification fails', async () => {
    const { updater, update, save } = fixture()
    update.download.mockRejectedValueOnce(new Error('signature mismatch'))
    await updater.checkForUpdates()
    await updater.install()
    expect(save).not.toHaveBeenCalled()
    expect(update.install).not.toHaveBeenCalled()
    expect(native.relaunch).not.toHaveBeenCalled()
    expect(updater.error.value).toContain('更新包校验失败')
    expect(updater.manualDownloadAvailable.value).toBe(true)
    expect(updater.busy.value).toBe(false)
  })

  it.each(['blocked', 'failed'] as const)(
    'preserves the download and prevents install when saving is %s',
    async (failure) => {
      const { updater, update, save } = fixture()
      if (failure === 'blocked') save.mockResolvedValueOnce(false)
      else save.mockRejectedValueOnce(new Error('磁盘已满'))
      await updater.checkForUpdates()
      await updater.install()
      expect(update.install).not.toHaveBeenCalled()
      expect(native.relaunch).not.toHaveBeenCalled()
      expect(updater.error.value).not.toBe('')
      expect(updater.manualDownloadAvailable.value).toBe(false)
      await updater.install()
      expect(update.install).toHaveBeenCalledOnce()
      // A thrown save may require re-downloading; a blocked task reuses the bytes.
      expect(update.download).toHaveBeenCalledTimes(failure === 'blocked' ? 1 : 2)
    },
  )

  it('restarts an installed update without re-installing, saving any new edits first', async () => {
    const { updater, update, save } = fixture()
    native.relaunch.mockRejectedValueOnce(new Error('restart unavailable'))
    await updater.checkForUpdates()
    await updater.install()
    expect(updater.status.value).toBe('installed')
    expect(updater.error.value).toContain('更新已安装')
    save.mockResolvedValueOnce(false)
    await updater.restart()
    expect(native.relaunch).toHaveBeenCalledOnce()
    await updater.restart()
    expect(native.relaunch).toHaveBeenCalledTimes(2)
    expect(update.install).toHaveBeenCalledOnce()
    expect(save).toHaveBeenCalledTimes(3)
  })

  it('does not duplicate checks or installs while requests are running', async () => {
    const { updater, update } = fixture()
    const pending = deferred<Update | null>()
    native.check.mockReturnValueOnce(pending.promise)
    const checking = updater.checkForUpdates()
    await updater.checkForUpdates()
    await vi.waitFor(() => expect(native.check).toHaveBeenCalledOnce())
    pending.resolve(update as unknown as Update)
    await checking
    const saving = deferred<boolean>()
    const second = useUpdater({ beforeInstall: () => saving.promise })
    await second.checkForUpdates()
    const installing = second.install()
    await vi.waitFor(() => expect(second.status.value).toBe('preparing'))
    second.close()
    expect(second.visible.value).toBe(true)
    await second.install()
    expect(update.download).toHaveBeenCalledOnce()
    saving.resolve(true)
    await installing
  })

  it('handles unknown download length without displaying a false percentage', async () => {
    const { updater, update } = fixture()
    update.download.mockImplementationOnce(async (onEvent) => {
      onEvent({ event: 'Started', data: {} })
      onEvent({ event: 'Progress', data: { chunkLength: 80 } })
    })
    await updater.checkForUpdates()
    await updater.install()
    expect(updater.progress.value).toBeUndefined()
  })

  it('releases checks that complete after the app is disposed without notifying', async () => {
    const { updater, update } = fixture()
    const pending = deferred<Update | null>()
    native.check.mockReturnValueOnce(pending.promise)
    const checking = updater.checkForUpdates(false)
    await vi.waitFor(() => expect(native.check).toHaveBeenCalledOnce())
    updater.dispose()
    pending.resolve(update as unknown as Update)
    await checking
    expect(update.close).toHaveBeenCalledOnce()
    expect(messages.value).toHaveLength(0)
  })

  it('opens the fixed download page only after the user clicks, including after a toast is dismissed', async () => {
    const { updater } = fixture()
    native.check.mockRejectedValueOnce(new Error('offline'))
    await updater.checkForUpdates()
    expect(native.invoke).not.toHaveBeenCalledWith('open_update_download')
    clearMessages()
    const wrapper = mount(UpdateSettings, { props: { updater } })
    try {
      const link = wrapper.get('.update-manual-download a')
      expect(link.attributes('href')).toBe(UPDATE_DOWNLOAD_URL)
      expect(wrapper.text()).toContain('手动安装前，请先保存并退出当前应用')
      await link.trigger('click')
      expect(native.invoke).toHaveBeenCalledWith('open_update_download')
    } finally {
      wrapper.unmount()
    }
  })

  it('offers a manual download after installation fails without restarting', async () => {
    const { updater, update } = fixture()
    update.install.mockRejectedValueOnce(new Error('permission denied'))
    await updater.checkForUpdates()
    await updater.install()
    expect(updater.manualDownloadAvailable.value).toBe(true)
    expect(native.relaunch).not.toHaveBeenCalled()
  })

  it('retains the address if the system browser cannot be opened and permits retry', async () => {
    const { updater } = fixture()
    native.invoke.mockRejectedValueOnce(new Error('browser unavailable'))
    await updater.openManualDownload()
    expect(updater.manualDownloadError.value).toContain('复制下载地址')
    expect(messages.value[0]?.text).toContain(UPDATE_DOWNLOAD_URL)
    expect(updater.manualDownloadOpening.value).toBe(false)
    await updater.openManualDownload()
    expect(updater.manualDownloadError.value).toBe('')
  })

  it('does not offer fallback downloads when the version is confirmed current', async () => {
    const { updater } = fixture()
    native.check.mockResolvedValueOnce(null)
    await updater.checkForUpdates()
    expect(updater.status.value).toBe('latest')
    expect(updater.manualDownloadAvailable.value).toBe(false)
    expect(messages.value[0]?.tone).toBe('success')
  })
})
