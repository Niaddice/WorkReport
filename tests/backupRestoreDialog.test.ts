// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import BackupRestoreDialog from '../src/components/BackupRestoreDialog.vue'
import * as backupLib from '../src/lib/backupMigration'
import type { Document, Entry } from '../src/lib/workspace'

describe('BackupRestoreDialog', () => {
  let wrappers: ReturnType<typeof mount>[] = []

  const sampleEntries: Entry[] = [{ date: '20261026', filename: '20261026.md', format: 'md' }]

  beforeEach(() => {
    wrappers = []
  })

  afterEach(() => {
    wrappers.forEach((w) => w.unmount())
    wrappers = []
    document.body.innerHTML = ''
    vi.restoreAllMocks()
  })

  function mountDialog(props: Partial<InstanceType<typeof BackupRestoreDialog>['$props']> = {}) {
    const wrapper = mount(BackupRestoreDialog, {
      attachTo: document.body,
      props: {
        visible: true,
        workspaceName: 'TestWorkspace',
        entries: sampleEntries,
        readEntry: vi.fn().mockResolvedValue({
          date: '20261026',
          filename: '20261026.md',
          format: 'md',
          content: '测试日报',
          revision: 'r1',
        } as Document),
        createEntry: vi.fn().mockResolvedValue({} as Document),
        saveEntry: vi.fn().mockResolvedValue({} as Document),
        ...props,
      },
    })
    wrappers.push(wrapper)
    return wrapper
  }

  it('renders dialog and displays workspace export information', async () => {
    const wrapper = mountDialog()
    await flushPromises()

    const dialog = document.querySelector('#backup-restore-dialog')
    expect(dialog).not.toBeNull()
    expect(document.querySelector('.backup-title')?.textContent).toContain('工作空间备份与数据迁移')
    expect(document.querySelector('.info-value')?.textContent).toContain('TestWorkspace')

    // Close button test
    const closeBtn = document.querySelector<HTMLButtonElement>('.backup-close-btn')
    closeBtn?.click()
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('triggers backup export and file download on button click', async () => {
    const downloadSpy = vi.spyOn(backupLib, 'downloadBackupFile').mockImplementation(() => {})
    mountDialog()
    await flushPromises()

    const exportBtn = document.querySelector<HTMLButtonElement>('.backup-primary-btn')
    expect(exportBtn).not.toBeNull()
    exportBtn?.click()
    await flushPromises()

    expect(downloadSpy).toHaveBeenCalled()
    expect(document.querySelector('.export-success-banner')).not.toBeNull()
  })

  it('switches to restore tab and shows file dropzone', async () => {
    mountDialog()
    await flushPromises()

    const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>('.backup-tab-btn'))
    const restoreTab = tabs.find((t) => t.textContent?.includes('从备份包恢复'))
    expect(restoreTab).not.toBeUndefined()
    restoreTab?.click()
    await flushPromises()

    expect(document.querySelector('.backup-dropzone')).not.toBeNull()
    expect(document.querySelector('.dropzone-copy')?.textContent).toContain('点击选择本地备份文件')
  })
})
