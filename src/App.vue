<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { open as pickDirectory } from '@tauri-apps/plugin-dialog'
import { PhCircleNotch, PhFolder } from '@phosphor-icons/vue'
import AppToolbar from './components/AppToolbar.vue'
import AppRail from './components/AppRail.vue'
import SkinDialog from './components/SkinDialog.vue'
import SettingsView from './components/SettingsView.vue'
import WorkspaceGate from './components/WorkspaceGate.vue'
import ConfirmDialog from './components/ConfirmDialog.vue'
import SettingsLeaveDialog from './components/SettingsLeaveDialog.vue'
import WorkspaceSwitchDialog from './components/WorkspaceSwitchDialog.vue'
import MessageHost from './components/MessageHost.vue'
import { api, type Document, type FileFormat } from './lib/workspace'
import { historyShortcut } from './lib/shortcuts'
import { useJournal } from './lib/useJournal'
import { aiSummarySource, plainTextSummary } from './lib/summary'
import { useAi } from './lib/ai'
import ModuleBoard from './components/ModuleBoard.vue'
import LayoutDialog from './components/LayoutDialog.vue'
import type { DropSide, ModuleId } from './modules/types'
import { MODULE_CONTEXT, type CalendarHandle, type EditorHandle, type ModuleActions } from './modules/context'
import { useModuleLayout } from './modules/useModuleLayout'
import type { SkinId } from './lib/skins'
import { useAppearance } from './lib/useAppearance'
import { sameWeekRange, useExcel } from './lib/useExcel'
import { useSummaryOutput } from './lib/useSummaryOutput'
import { settingsApi, type WorkspaceSettings } from './lib/settingsDraft'
import {
  databaseApi,
  cloneDatabaseSettings,
  DEFAULT_DATABASE_SETTINGS,
  type DatabaseTransferResult,
} from './lib/database'
import { dateKey } from './lib/calendar'
import { useHolidays } from './lib/holidays'
import { useHolidayReminder } from './lib/useHolidayReminder'
import { clearMessages, dismissMessage, pushMessage } from './lib/messages'
import { useJournalPreview } from './lib/useJournalPreview'
import JournalPreviewWindow from './components/JournalPreviewWindow.vue'
import JournalPreviewContent from './components/JournalPreviewContent.vue'

const nativeApp = isTauri()
const journal = useJournal()
const {
  workspace,
  entries,
  selectedDate,
  document: activeDocument,
  isNewDraft,
  dirty,
  busy,
  saving,
  error,
  backupWarning,
  backupRetrying,
} = journal
const ai = useAi()
const appearance = useAppearance()
const excel = useExcel()
const summaryOutput = useSummaryOutput()
const holidays = useHolidays()
const { state: holidayState, loading: holidayLoading, syncing: holidaySyncing, error: holidayError } = holidays
const holidayReady = ref(false)
const { settings: summaryOutputSettings } = summaryOutput
const summaryWorkspacePath = ref('')
const { settings: excelSettings, preparing: excelPreparing, exporting: excelExporting } = excel
const { scale: appearanceScale, rail: appearanceRail, loading: appearanceLoading, error: appearanceError } = appearance
watch(
  appearanceScale,
  (scale) => {
    document.documentElement.style.setProperty('--font-scale', String(scale / 100))
  },
  { immediate: true, flush: 'sync' },
)
// Settings live inside the workspace, so a newly opened workspace brings its own values.
watch(workspace, (value, previous) => {
  if (!value || value.path === previous?.path) return
  void loadWorkspaceSettings(true)
})
const {
  settings: aiSettings,
  error: aiSettingsError,
  defaultProvider,
  summaryVisible,
  summaryDates,
  summaryResult,
  summaryMode,
} = ai
watch(summaryResult, () => summaryOutput.clearExport())
const page = ref<'home' | 'settings'>('home')
const recent = ref<string | null>(null)
const startupPending = ref(nativeApp)
const picking = ref(false)
const workspaceSwitchDialog = ref<InstanceType<typeof WorkspaceSwitchDialog>>()
const pendingWorkspaceSwitch = ref<{ path: string; sourcePath: string; sourceName: string } | null>(null)
const workspaceSwitchError = ref('')
const openingDirectory = ref(false)
const requestingAi = ref(false)
const excelBusy = computed(() => excelPreparing.value || excelExporting.value)
const summarySelectionDates = ref<string[]>([])
const refreshRevision = ref(0)
const settingsView = ref<InstanceType<typeof SettingsView>>()
const databaseSettings = ref(cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS))
const databaseLoading = ref(false)
const settingsDatabaseError = ref('')
const databaseActivity = ref(false)
const storageRefreshing = ref(false)
const databaseBusy = computed(() => databaseActivity.value || storageRefreshing.value || backupRetrying.value)
const storageMode = computed(() => (!databaseSettings.value.enabled ? 'files' : databaseSettings.value.mode))

// 新建日报沿用「数据存储」里选定的默认格式；已有日报不受影响。
watch(
  () => databaseSettings.value.defaultFormat,
  (format) => journal.setDefaultFormat(format),
  { immediate: true },
)

const workspaceSettings = computed<WorkspaceSettings>(() => ({
  appearance: { scale: appearanceScale.value, rail: appearanceRail.value },
  ai: aiSettings.value,
  excel: excelSettings.value,
  summary: summaryOutputSettings.value,
  database: databaseSettings.value,
}))
const settingsReading = ref(false)
const settingsSaving = ref(false)
const settingsReady = ref(false)
const settingsError = ref('')
const settingsLoading = computed(
  () =>
    settingsReading.value ||
    appearanceLoading.value ||
    excel.loading.value ||
    summaryOutput.loading.value ||
    databaseLoading.value,
)
let settingsLoadRevision = 0
type SettingsExit = 'home' | 'workspace' | 'close'
const settingsLeaveDialog = ref<InstanceType<typeof SettingsLeaveDialog>>()
const skinDialog = ref<InstanceType<typeof SkinDialog>>()
const pendingSettingsExit = ref<SettingsExit | null>(null)
const settingsLeaveBusy = ref(false)
const settingsLeaveError = ref('')
const confirmDialog = ref<InstanceType<typeof ConfirmDialog>>()
const modalKind = ref<'reload' | 'format' | 'restore'>('format')
const targetFormat = ref<FileFormat>('md')
const editorPreview = ref(false)
const layoutError = ref('')
const moduleLayout = useModuleLayout({
  onError: (message) => {
    layoutError.value = message
  },
  workspacePath: () => workspace.value?.path ?? null,
})
const { layout: layoutTree, items: layoutItems, skin: activeSkin, savedLayouts } = moduleLayout
const layoutDialog = ref<InstanceType<typeof LayoutDialog>>()
const moduleBoard = ref<InstanceType<typeof ModuleBoard>>()
const calendarHandle = ref<CalendarHandle | null>(null)
const editorHandle = ref<EditorHandle | null>(null)

watch(
  activeSkin,
  (skin) => {
    document.documentElement.dataset.skin = skin
  },
  { immediate: true },
)
watch(
  () => workspace.value?.path,
  (path) => {
    // 工作空间变了：先清掉旧空间布局，避免加载返回前把旧布局写进新空间。
    moduleLayout.resetLocal()
    if (path) void moduleLayout.load()
  },
)
function openLayoutDialog() {
  layoutDialog.value?.open()
}
function closeLayoutDialog() {
  void layoutDialog.value?.close()
}
function removeModule(id: ModuleId) {
  moduleLayout.remove(id)
}
function editorPreviewAnchor() {
  return moduleBoard.value?.getModuleRect('editor') ?? null
}
async function activateModule(id: ModuleId) {
  if (page.value !== 'home') {
    await navigate('home')
    if ((page.value as string) !== 'home') return
  }
  if (!layoutItems.value.some((item) => item.id === id)) moduleLayout.add(id)
  await nextTick()
  moduleBoard.value?.reveal(id)
}
async function openLayoutFromRail() {
  if (page.value !== 'home') {
    await navigate('home')
    if ((page.value as string) !== 'home') return
  }
  openLayoutDialog()
}
function splitModule(dragged: ModuleId, target: ModuleId, side: DropSide) {
  moduleLayout.split(dragged, target, side)
}
function insertModuleAtGutter(dragged: ModuleId, path: number[]) {
  moduleLayout.insertAtGutter(dragged, path)
}
function moveModuleToEdge(dragged: ModuleId, side: 'left' | 'right') {
  moduleLayout.moveToEdge(dragged, side)
}
function resizeModules(path: number[], ratio: number) {
  moduleLayout.resize(path, ratio)
}
function resetLayout() {
  moduleLayout.reset()
}
function saveCurrentLayout(name: string) {
  moduleLayout.saveLayout(name)
}
function applySavedLayout(name: string) {
  moduleLayout.applySavedLayout(name)
}
function removeSavedLayout(name: string) {
  moduleLayout.removeSavedLayout(name)
}
function openSkinDialog() {
  skinDialog.value?.open()
}
function closeSkinDialog() {
  void skinDialog.value?.close()
}
/** 目录模块里点单篇表示聚焦这一天：先解除框选，避免总结 / Excel 仍按旧范围统计。 */
async function selectReport(date: string) {
  if (date === selectedDate.value) return
  if (rangeDates.value.length) {
    calendarHandle.value?.clearRange()
    clearRange()
  }
  await selectDate(date)
}
function applySkin(id: SkinId) {
  moduleLayout.setSkin(id)
}
const rangeDates = ref<string[]>([])
const rangeDocuments = ref<Document[]>([])
const rangeLoading = ref(false)
const rangeError = ref('')
const journalPreview = useJournalPreview(journal, {
  dates: rangeDates,
  documents: rangeDocuments,
  loading: rangeLoading,
  error: rangeError,
})
const excelBlockedReason = computed(() => {
  if (!sameWeekRange(summarySelectionDates.value))
    return '仅支持同一周内的日期范围（周一至周日），包括没有日报的所选日期。'
  if (!defaultProvider.value) return '请先在设置中配置并选择默认 AI。'
  if (!excelSettings.value.templatePath || !excelSettings.value.prompt.trim() || !excelSettings.value.outputDirectory)
    return '请先完成 Excel 输出设置。'
  return ''
})
let rangeRevision = 0
const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent)
const now = ref(dayjs())
const today = computed(() => dateKey(now.value))
const holidayDate = computed(() => now.value.format('YYYY-MM-DD'))
const { visible: holidayReminderVisible, dismiss: dismissHolidayReminder } = useHolidayReminder(
  holidayState,
  holidayDate,
  holidayReady,
)
const holidayCurrentFailure = computed(() => {
  const record = holidayState.value.years.find((item) => item.year === now.value.year())
  return holidayState.value.failures.some(
    (item) =>
      item.year === now.value.year() && (!record || Date.parse(item.attemptedAt) >= Date.parse(record.syncedAt)),
  )
})
const holidayReminderText = computed(
  () =>
    `${now.value.year()} 年放假信息${holidayCurrentFailure.value ? '上次同步失败' : '尚未同步成功'}，请手动同步到日历。`,
)
const recentName = computed(() => recent.value?.split(/[\\/]/).filter(Boolean).at(-1))
const greeting = computed(() => (now.value.hour() < 12 ? '上午好' : now.value.hour() < 18 ? '下午好' : '晚上好'))
const documentKey = computed(() => `${workspace.value?.path ?? ''}/${selectedDate.value}`)
const backups = ref<Record<string, string>>({})
const canRestore = computed(() => backups.value[documentKey.value] !== undefined)
let unlistenClose: (() => void) | undefined
let unlistenHistory: (() => void) | undefined
let historyMenuAvailable = false
let clockTimer: ReturnType<typeof setInterval> | undefined
let appDisposed = false

async function restoreRecentWorkspace() {
  try {
    const path = await api.recent()
    if (appDisposed) return
    recent.value = path
    if (path) await openSelectedWorkspace(path)
  } catch (cause) {
    if (!appDisposed) journal.reportError(cause)
  } finally {
    if (!appDisposed) startupPending.value = false
  }
}

async function navigate(target: 'home' | 'settings') {
  if (
    busy.value ||
    picking.value ||
    settingsSaving.value ||
    settingsReading.value ||
    databaseBusy.value ||
    settingsLeaveBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value ||
    page.value === target
  )
    return
  if (target === 'home' && requestSettingsExit('home')) return
  if (!(await journal.flush())) return
  page.value = target
  if (target === 'settings') {
    await loadWorkspaceSettings(false)
    await nextTick()
    settingsView.value?.reset()
  }
}
async function toggleSettings() {
  if (page.value !== 'settings') {
    await navigate('settings')
    return
  }
  if (
    !settingsView.value ||
    busy.value ||
    picking.value ||
    settingsLoading.value ||
    settingsSaving.value ||
    settingsLeaveBusy.value ||
    databaseBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value
  )
    return
  settingsLeaveBusy.value = true
  try {
    if (!settingsView.value.unchanged && !(await settingsView.value.save())) return
    if (await journal.flush()) page.value = 'home'
  } finally {
    settingsLeaveBusy.value = false
  }
}
async function openHolidaySettings() {
  await navigate('settings')
  if (page.value === 'settings') await settingsView.value?.navigate('holidays')
}
async function openAiSettings() {
  await navigate('settings')
  if ((page.value as string) === 'settings') await settingsView.value?.navigate('ai')
}
async function openExcelSettings() {
  await navigate('settings')
  if ((page.value as string) === 'settings') await settingsView.value?.navigate('excel')
}
async function loadWorkspaceSettings(reload: boolean): Promise<boolean> {
  const revision = ++settingsLoadRevision
  settingsReading.value = true
  settingsReady.value = false
  settingsError.value = ''
  const results = await Promise.all([
    reload ? appearance.reload() : appearance.load(),
    reload ? ai.reload() : ai.load(),
    reload ? excel.reload() : excel.load(),
    reload ? summaryOutput.reload() : summaryOutput.load(),
    loadDatabaseSettings(),
  ])
  if (revision !== settingsLoadRevision) return false
  settingsReading.value = false
  settingsReady.value = results.every(Boolean)
  if (!settingsReady.value)
    settingsError.value =
      [
        appearanceError.value,
        aiSettingsError.value,
        excel.error.value,
        summaryOutput.error.value,
        settingsDatabaseError.value,
      ]
        .filter(Boolean)
        .join('；') || '读取设置失败，请重新进入设置后重试。'
  return settingsReady.value
}
async function loadDatabaseSettings(): Promise<boolean> {
  const path = workspace.value?.path
  databaseLoading.value = true
  settingsDatabaseError.value = ''
  try {
    const stored = nativeApp ? await databaseApi.load() : cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS)
    if (workspace.value?.path !== path) return false
    databaseSettings.value = cloneDatabaseSettings(stored)
    return true
  } catch (cause) {
    if (workspace.value?.path === path) settingsDatabaseError.value = `读取数据库设置失败：${String(cause)}`
    return false
  } finally {
    if (workspace.value?.path === path) databaseLoading.value = false
  }
}
async function onDatabaseTransferred(result: DatabaseTransferResult) {
  if (result.message) pushMessage({ tone: 'success', text: result.message })
  storageRefreshing.value = true
  try {
    databaseSettings.value = cloneDatabaseSettings(result.settings)
    await nextTick()
    settingsView.value?.reset()
    clearRange()
    calendarHandle.value?.clearRange()
    await journal.reload()
  } finally {
    storageRefreshing.value = false
  }
}
async function saveWorkspaceSettings(value: WorkspaceSettings): Promise<boolean> {
  if (settingsSaving.value || databaseBusy.value) return false
  if (!workspace.value || !settingsReady.value) {
    settingsError.value = '请先打开工作空间并完整读取设置后重试。'
    return false
  }
  const path = workspace.value.path
  settingsSaving.value = true
  settingsError.value = ''
  try {
    if (!(await journal.flush())) {
      settingsError.value = '请先保存当前日报，再更改存储设置。'
      return false
    }
    const storageChanged = JSON.stringify(value.database) !== JSON.stringify(databaseSettings.value)
    const stored = await settingsApi.save(path, value)
    if (workspace.value?.path !== path) return false
    ai.acceptSaved(stored.ai)
    appearance.acceptSaved(stored.appearance)
    excel.acceptSaved(stored.excel)
    summaryOutput.acceptSaved(stored.summary)
    databaseSettings.value = cloneDatabaseSettings(stored.database)
    if (storageChanged) {
      clearRange()
      calendarHandle.value?.clearRange()
      await journal.reload()
    }
    return true
  } catch (cause) {
    settingsError.value = `保存设置失败：${String(cause)}`
    return false
  } finally {
    settingsSaving.value = false
  }
}
function requestSettingsExit(action: SettingsExit): boolean {
  if (page.value !== 'settings' || !settingsView.value || settingsView.value.unchanged) return false
  if (!pendingSettingsExit.value) {
    pendingSettingsExit.value = action
    settingsLeaveError.value = ''
    settingsLeaveDialog.value?.open()
  }
  return true
}
function cancelSettingsExit() {
  if (settingsLeaveBusy.value || settingsSaving.value) return
  pendingSettingsExit.value = null
  settingsLeaveError.value = ''
  void settingsLeaveDialog.value?.close()
}
async function finishSettingsExit(save: boolean) {
  if (!pendingSettingsExit.value || settingsLeaveBusy.value || settingsSaving.value) return
  settingsLeaveBusy.value = true
  settingsLeaveError.value = ''
  const action = pendingSettingsExit.value
  try {
    if (save && !(await settingsView.value?.save())) {
      settingsLeaveError.value = settingsError.value || '设置未保存，请继续编辑并检查标出的内容。'
      return
    }
    if (!save) settingsView.value?.reset()
    pendingSettingsExit.value = null
    const closing = settingsLeaveDialog.value?.close() ?? Promise.resolve(true)
    if (!(await closing)) return
    if (action === 'workspace') await performChooseWorkspace()
    else if (action === 'close') await closeWindow()
    else if (await journal.flush()) page.value = 'home'
  } finally {
    settingsLeaveBusy.value = false
  }
}
async function closeWindow() {
  if (excelExporting.value) return
  rangeRevision++
  ai.closeSummary()
  await excel.cancel()
  if (await journal.flush()) await getCurrentWindow().destroy()
}
async function openWorkspaceDirectory() {
  if (!workspace.value || openingDirectory.value) return
  openingDirectory.value = true
  try {
    await api.openDirectory()
  } catch (cause) {
    journal.reportError(cause)
  } finally {
    openingDirectory.value = false
  }
}
async function chooseWorkspace() {
  if (
    picking.value ||
    busy.value ||
    settingsSaving.value ||
    settingsReading.value ||
    databaseBusy.value ||
    settingsLeaveBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value
  )
    return
  if (requestSettingsExit('workspace')) return
  await performChooseWorkspace()
}
async function performChooseWorkspace() {
  if (!nativeApp) {
    journal.reportError('请在 Tauri 桌面应用中选择本地文件夹。使用 npm run app:dev 启动。')
    return
  }
  if (!(await journal.flush())) return
  picking.value = true
  try {
    const path = await pickDirectory({
      directory: true,
      multiple: false,
      title: '选择 WorkReport 工作空间',
      defaultPath: workspace.value?.path ?? recent.value ?? undefined,
    })
    if (appDisposed || typeof path !== 'string' || path === workspace.value?.path) return
    if (workspace.value) {
      pendingWorkspaceSwitch.value = { path, sourcePath: workspace.value.path, sourceName: workspace.value.name }
      workspaceSwitchError.value = ''
      await nextTick()
      workspaceSwitchDialog.value?.open()
    } else await openSelectedWorkspace(path)
  } catch (cause) {
    journal.reportError(cause)
  } finally {
    picking.value = false
  }
}
async function openSelectedWorkspace(path: string, syncSettingsFrom?: string) {
  const previous = workspace.value?.path
  const opened = await journal.open(path, syncSettingsFrom)
  if (!opened && (!workspace.value || workspace.value.path === previous)) return false
  calendarHandle.value?.clearRange()
  clearRange()
  recent.value = workspace.value?.path ?? path
  return true
}
function cancelWorkspaceSwitch() {
  if (picking.value || busy.value) return
  pendingWorkspaceSwitch.value = null
  workspaceSwitchError.value = ''
  void workspaceSwitchDialog.value?.close()
}
async function finishWorkspaceSwitch(sync: boolean) {
  const pending = pendingWorkspaceSwitch.value
  if (!pending || picking.value || busy.value) return
  picking.value = true
  workspaceSwitchError.value = ''
  try {
    if (!(await openSelectedWorkspace(pending.path, sync ? pending.sourcePath : undefined))) {
      workspaceSwitchError.value = error.value || '工作空间未切换，请检查后重试。'
      return
    }
    pendingWorkspaceSwitch.value = null
    await workspaceSwitchDialog.value?.close()
    if (sync) pushMessage({ tone: 'success', text: '设置已同步到新工作空间。' })
  } finally {
    picking.value = false
  }
}
function showConfirmation() {
  confirmDialog.value?.open()
}
async function selectDate(date: string) {
  if (excelBusy.value) return
  if (await journal.select(date)) editorHandle.value?.focus()
}
async function requestFormat(format: FileFormat) {
  if (isNewDraft.value) {
    if (await journal.switchFormat(format)) editorHandle.value?.focus()
    return
  }
  targetFormat.value = format
  modalKind.value = 'format'
  showConfirmation()
}
function requestReload() {
  modalKind.value = 'reload'
  showConfirmation()
}
function requestRestore() {
  modalKind.value = 'restore'
  showConfirmation()
}
async function confirmModal() {
  if (modalKind.value === 'format') {
    if (await journal.switchFormat(targetFormat.value)) {
      await confirmDialog.value?.close()
      editorHandle.value?.focus()
    }
  } else if (modalKind.value === 'restore') {
    const original = backups.value[documentKey.value]
    if (original !== undefined) {
      journal.edit(original)
      if (!(await journal.flush())) return
      delete backups.value[documentKey.value]
    }
    void confirmDialog.value?.close()
  } else {
    await journal.reload()
    void confirmDialog.value?.close()
  }
}
function cancelConfirm() {
  if (!busy.value) void confirmDialog.value?.close()
}
function clearRange() {
  excel.clearExport()
  rangeRevision++
  rangeDates.value = []
  rangeDocuments.value = []
  rangeLoading.value = false
  rangeError.value = ''
}
async function loadRange(dates: string[]) {
  if (excelBusy.value) return
  if (!dates.length) {
    clearRange()
    return
  }
  const revision = ++rangeRevision
  excel.clearExport()
  rangeDates.value = [...dates].sort()
  rangeDocuments.value = []
  rangeError.value = ''
  rangeLoading.value = true
  try {
    if (!(await journal.flush()) && revision === rangeRevision)
      rangeError.value = '当前草稿保存失败，合并视图可能不是最新内容。'
    if (revision !== rangeRevision) return
    const selected = entries.value
      .filter((entry) => dates.includes(entry.date))
      .sort((left, right) => right.date.localeCompare(left.date))
    if (!selected.length) return
    const documents = await Promise.all(selected.map((entry) => api.read(entry.date, entry.format)))
    if (revision !== rangeRevision) return
    rangeDocuments.value = documents.sort((left, right) => right.date.localeCompare(left.date))
  } catch (cause) {
    if (revision === rangeRevision) rangeError.value = String(cause)
  } finally {
    if (revision === rangeRevision) rangeLoading.value = false
  }
}
async function refreshHome() {
  if (excelBusy.value) return
  refreshRevision.value++
  if (rangeDates.value.length) await loadRange([...rangeDates.value])
  else await journal.refresh()
}
function performHistory(action: 'undo' | 'redo') {
  const active = document.activeElement
  const editable = active instanceof HTMLElement ? active.closest('input, textarea, [contenteditable="true"]') : null
  if (editable && !editable.closest('.markdown-editor')) {
    document.execCommand(action)
    return
  }
  // 总结和 Excel 模块也复用 open 的 dialog，只读内嵌区域不能阻断日报历史。
  if (
    page.value !== 'home' ||
    busy.value ||
    rangeDates.value.length ||
    document.querySelector('dialog[open]:not(.is-inline)')
  )
    return
  if (action === 'undo') journal.undo()
  else journal.redo()
}
function onHistoryKeydown(event: KeyboardEvent) {
  // On macOS the native Edit menu owns the key equivalents and forwards them
  // through the `history-action` event instead of dispatching keydown here.
  if (isMac && nativeApp && historyMenuAvailable) return
  if (
    page.value !== 'home' ||
    busy.value ||
    rangeDates.value.length ||
    document.querySelector('dialog[open]:not(.is-inline)')
  )
    return
  const target = event.target
  if (
    target instanceof HTMLElement &&
    target !== document.body &&
    target.closest('input, textarea, [contenteditable="true"]') &&
    !target.closest('.markdown-editor')
  )
    return
  const action = historyShortcut(event, isMac)
  if (!action) return
  event.preventDefault()
  performHistory(action)
}
async function requestSummary(dates: string[]) {
  if (requestingAi.value || busy.value || excelBusy.value) return
  const workspacePath = workspace.value?.path
  const selectionRevision = rangeRevision
  requestingAi.value = true
  try {
    if (!(await journal.flush())) return
    if (workspace.value?.path !== workspacePath || page.value !== 'home' || selectionRevision !== rangeRevision) return
    const selected = entries.value
      .filter((entry) => dates.includes(entry.date))
      .sort((left, right) => left.date.localeCompare(right.date))
    if (!selected.length) {
      closeSummary()
      return
    }
    const documents = await Promise.all(selected.map((entry) => api.read(entry.date, entry.format)))
    if (workspace.value?.path !== workspacePath || page.value !== 'home' || selectionRevision !== rangeRevision) return
    if (!documents.some((document) => document.content.trim())) {
      closeSummary()
      return
    }
    const aiContent = aiSummarySource(documents)
    const directContent = plainTextSummary(documents)
    summaryWorkspacePath.value = workspacePath ?? ''
    summarySelectionDates.value = [...dates]
    summaryOutput.clearExport()
    if (summaryVisible.value) ai.closeSummary()
    ai.openSummary(
      { ai: aiContent, direct: directContent },
      documents.map((document) => document.date),
      dates,
    )
  } catch (cause) {
    if (workspace.value?.path === workspacePath && page.value === 'home' && selectionRevision === rangeRevision)
      journal.reportError(cause)
  } finally {
    requestingAi.value = false
  }
}
function closeSummary() {
  ai.closeSummary()
  summaryOutput.clearExport()
}
function exportSummary(format: FileFormat) {
  if (
    !summaryVisible.value ||
    ai.summaryGenerating.value ||
    summaryOutput.exporting.value ||
    excelBusy.value ||
    workspace.value?.path !== summaryWorkspacePath.value
  )
    return
  void summaryOutput.exportSummary(
    summaryWorkspacePath.value,
    [...summaryDates.value],
    summaryResult.value,
    format,
    summaryMode.value,
  )
}
async function prepareExcel(dates: string[]) {
  if (busy.value || excelBusy.value || !workspace.value) return
  const workspacePath = workspace.value.path
  summaryWorkspacePath.value = workspacePath
  summarySelectionDates.value = [...dates].sort()
  excel.clearExport()
  excel.exportError.value = ''
  if (!excelSettings.value.enabled) {
    excel.exportError.value = '请先在设置中启用 Excel 输出。'
    return
  }
  const blockedReason = excelBlockedReason.value
  if (blockedReason) {
    excel.exportError.value = blockedReason
    return
  }
  await excel.prepareSelection(workspacePath, summarySelectionDates.value)
}
async function confirmExcelPreview() {
  if (excelExporting.value || excelBusy.value || !excel.preview.value) return
  if (summaryWorkspacePath.value !== workspace.value?.path) {
    excel.exportError.value = '工作空间已切换，请重新生成 Excel 预览。'
    return
  }
  await excel.confirmPreview(summaryWorkspacePath.value)
}
function discardExcelPreview() {
  if (excelExporting.value) return
  void excel.discardPreview()
}
async function openExcelOutputDirectory() {
  if (!nativeApp) return
  try {
    await invoke<void>('open_excel_output_directory')
  } catch (cause) {
    excel.exportError.value = `无法打开 Excel 输出目录：${String(cause)}`
  }
}
async function startAiSummary() {
  if (requestingAi.value || ai.summaryGenerating.value || excelBusy.value || !summaryVisible.value) return
  requestingAi.value = true
  try {
    const loaded = await ai.load()
    if (!loaded) {
      journal.reportError(`无法读取 AI 设置：${aiSettingsError.value}`)
      return
    }
    if (!defaultProvider.value) {
      journal.reportError('请先在设置中配置并选择默认 AI。')
      void navigate('settings')
      return
    }
    void ai.generateSummary()
  } finally {
    requestingAi.value = false
  }
}
const moduleDisabled = computed(() => busy.value || requestingAi.value)
const moduleActions: ModuleActions = {
  selectDate,
  selectReport,
  loadRange,
  requestSummary,
  generateSummary: () => {
    void startAiSummary()
  },
  directSummary: ai.directSummary,
  closeSummary,
  exportSummary,
  prepareExcel,
  confirmExcel: confirmExcelPreview,
  discardExcelPreview,
  openExcelOutputDirectory,
  requestFormat,
  requestReload,
  requestRestore,
  openAiSettings: () => {
    void openAiSettings()
  },
  openExcelSettings: () => {
    void openExcelSettings()
  },
  openHolidaySettings: () => {
    void openHolidaySettings()
  },
  openWorkspaceDirectory,
  syncHolidays: (year: number) => {
    void syncHolidays(year)
  },
}
provide(MODULE_CONTEXT, {
  journal,
  ai,
  holidays,
  excel,
  summaryOutput,
  summarySelectionDates,
  excelBlockedReason,
  now,
  today,
  storageMode,
  canRestore,
  disabled: moduleDisabled,
  rangeDates,
  rangeDocuments,
  rangeLoading,
  rangeError,
  editorPreview,
  journalPreview,
  calendar: calendarHandle,
  editor: editorHandle,
  actions: moduleActions,
})
const messageIds: { backup: number; layout: number; holiday: number } = { backup: 0, layout: 0, holiday: 0 }

async function syncHolidays(year: number) {
  if (await holidays.sync(year)) pushMessage({ tone: 'success', text: `${year} 年放假信息已同步` })
}

watch(error, (value) => {
  if (!value) return
  pushMessage({
    tone: 'error',
    text: value,
    duration: dirty.value ? 0 : undefined,
    actions: dirty.value ? [{ label: '重试保存', run: () => void journal.flush() }] : undefined,
  })
})
watch(backupWarning, (value) => {
  if (value) {
    if (messageIds.backup) dismissMessage(messageIds.backup)
    messageIds.backup = pushMessage({
      tone: 'warning',
      text: value,
      duration: 0,
      actions: [{ label: '重试数据库备份', run: () => void journal.retryBackup() }],
    })
  } else if (messageIds.backup) {
    dismissMessage(messageIds.backup)
    messageIds.backup = 0
  }
})
watch(layoutError, (value) => {
  if (value) {
    if (messageIds.layout) dismissMessage(messageIds.layout)
    messageIds.layout = pushMessage({ tone: 'error', text: value })
  } else if (messageIds.layout) {
    dismissMessage(messageIds.layout)
    messageIds.layout = 0
  }
})
watch(
  () => summaryOutput.result.value,
  (value) => {
    if (!value?.filename) return
    pushMessage({
      tone: 'success',
      text: `总结已导出：${value.filename}`,
      duration: 5000,
      actions: [
        {
          label: '打开目标目录',
          run: () => {
            const path = workspace.value?.path
            if (!path) return
            void summaryOutput.openDirectory(path).then((opened) => {
              if (!opened && summaryOutput.openError.value)
                pushMessage({ tone: 'error', text: summaryOutput.openError.value })
            })
          },
        },
      ],
    })
  },
)
watch(
  () => excel.result.value,
  (value) => {
    if (value?.filename) pushMessage({ tone: 'success', text: `Excel 周报已生成：${value.filename}` })
  },
)
watch(
  [holidayReminderVisible, holidayReminderText],
  ([visible, text]) => {
    if (visible) {
      if (messageIds.holiday) dismissMessage(messageIds.holiday)
      messageIds.holiday = pushMessage({
        tone: 'warning',
        title: '放假信息提醒',
        text,
        duration: 0,
        actions: [
          {
            label: '前往同步',
            run: () => {
              dismissHolidayReminder()
              void openHolidaySettings()
            },
          },
          { label: '今天不再提醒', run: () => dismissHolidayReminder() },
        ],
      })
    } else if (messageIds.holiday) {
      dismissMessage(messageIds.holiday)
      messageIds.holiday = 0
    }
  },
  { immediate: true },
)
function beforeUnload(event: BeforeUnloadEvent) {
  if (
    dirty.value ||
    saving.value ||
    settingsSaving.value ||
    databaseBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value ||
    (page.value === 'settings' && !settingsView.value?.unchanged)
  ) {
    event.preventDefault()
    event.returnValue = ''
  }
}
function onFocus() {
  now.value = dayjs()
  if (
    !dirty.value &&
    !saving.value &&
    !error.value &&
    !confirmDialog.value?.isOpen() &&
    !picking.value &&
    !excelBusy.value &&
    !rangeDates.value.length &&
    !databaseBusy.value
  )
    void journal.refresh()
}
onMounted(async () => {
  window.addEventListener('beforeunload', beforeUnload)
  window.addEventListener('focus', onFocus)
  window.addEventListener('keydown', onHistoryKeydown)
  clockTimer = setInterval(() => {
    now.value = dayjs()
  }, 60_000)
  void appearance.load()
  void excel.load()
  void holidays.load().finally(() => {
    holidayReady.value = true
  })
  if (nativeApp) {
    void ai.load()
    void restoreRecentWorkspace()
    try {
      unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
        event.preventDefault()
        if (pendingWorkspaceSwitch.value) {
          cancelWorkspaceSwitch()
          return
        }
        if (
          busy.value ||
          picking.value ||
          settingsSaving.value ||
          settingsReading.value ||
          databaseBusy.value ||
          settingsLeaveBusy.value ||
          summaryOutput.exporting.value
        )
          return
        if (requestSettingsExit('close')) return
        await closeWindow()
      })
      unlistenHistory = await listen<string>('history-action', (event) => {
        if (event.payload === 'undo' || event.payload === 'redo') performHistory(event.payload)
      })
      historyMenuAvailable = true
    } catch (cause) {
      journal.reportError(cause)
    }
  }
})
onBeforeUnmount(() => {
  appDisposed = true
  rangeRevision++
  settingsLoadRevision++
  window.removeEventListener('beforeunload', beforeUnload)
  window.removeEventListener('focus', onFocus)
  window.removeEventListener('keydown', onHistoryKeydown)
  clearInterval(clockTimer)
  unlistenClose?.()
  unlistenHistory?.()
  ai.closeSummary()
  void excel.cancel()
  journal.dispose()
  clearMessages()
})
</script>

<template>
  <div class="app-shell" :class="{ 'has-native-titlebar': nativeApp && isMac, 'rail-left': appearanceRail === 'left' }">
    <div class="app-main">
      <AppToolbar
        :page="page"
        :native-titlebar="nativeApp && isMac"
        :greeting="greeting"
        :date-label="now.format('M 月 D 日 dddd')"
        :has-workspace="!!workspace"
        :busy="busy"
        :picking="picking"
        :excel-busy="excelBusy"
        :settings-saving="settingsSaving"
        :settings-reading="settingsReading"
        :settings-leave-busy="settingsLeaveBusy"
        :database-busy="databaseBusy"
        :refresh-revision="refreshRevision"
        @navigate="navigate"
        @refresh="refreshHome"
      />
      <div class="app-content">
        <main v-if="startupPending" class="workspace-loading" role="status" aria-live="polite">
          <PhCircleNotch class="spin" :size="18" aria-hidden="true" /><span>正在打开工作空间…</span>
        </main>
        <Transition name="page-switch">
          <div v-show="page === 'settings'" class="settings-viewport" :inert="page !== 'settings'">
            <SettingsView
              ref="settingsView"
              :key="workspace?.path ?? 'no-workspace'"
              :workspace="workspace"
              :picking="picking || busy || excelBusy || settingsSaving"
              :bundle="workspaceSettings"
              :loading="settingsLoading"
              :saving="settingsSaving"
              :database-busy="databaseBusy"
              :before-database-transfer="journal.flush"
              :error="settingsError"
              :save="saveWorkspaceSettings"
              :holidays="holidayState"
              :holiday-loading="holidayLoading"
              :holiday-syncing="holidaySyncing"
              :holiday-error="holidayError"
              :current-year="now.year()"
              @database-busy="databaseActivity = $event"
              @database-transferred="onDatabaseTransferred"
              @sync-holidays="syncHolidays"
              @back="navigate('home')"
              @workspace="chooseWorkspace"
            />
          </div>
        </Transition>
        <Transition name="page-switch">
          <main v-if="workspace && !startupPending" v-show="page === 'home'" class="home" :inert="page !== 'home'">
            <ModuleBoard
              ref="moduleBoard"
              :layout="layoutTree"
              :workspace-path="workspace.path"
              @split="splitModule"
              @insert="insertModuleAtGutter"
              @edge="moveModuleToEdge"
              @resize="resizeModules"
              @remove="removeModule"
            />
            <footer class="home-footer">
              <span :title="workspace.path">
                <button
                  class="workspace-folder-button"
                  aria-label="打开工作空间目录"
                  title="打开工作空间目录"
                  :disabled="openingDirectory"
                  @click="openWorkspaceDirectory"
                >
                  <PhFolder :size="15" />
                </button>
                <span class="workspace-path-text">{{ workspace.path }}</span>
              </span>
              <span>每一天，一份工作记录</span>
            </footer>
          </main>
        </Transition>
        <Transition name="page-switch">
          <WorkspaceGate
            v-if="!startupPending && !workspace && page === 'home'"
            :picking="picking"
            :busy="busy"
            :recent="recent"
            :recent-name="recentName"
            :error="error"
            @choose="chooseWorkspace"
            @open="openSelectedWorkspace"
          />
        </Transition>
      </div>
      <SettingsLeaveDialog
        ref="settingsLeaveDialog"
        :action="pendingSettingsExit"
        :busy="settingsLeaveBusy"
        :saving="settingsSaving"
        :error="settingsLeaveError"
        @cancel="cancelSettingsExit"
        @discard="finishSettingsExit(false)"
        @save="finishSettingsExit(true)"
      />
      <WorkspaceSwitchDialog
        ref="workspaceSwitchDialog"
        :source-name="pendingWorkspaceSwitch?.sourceName ?? ''"
        :target-path="pendingWorkspaceSwitch?.path ?? ''"
        :busy="picking || busy"
        :error="workspaceSwitchError"
        @cancel="cancelWorkspaceSwitch"
        @keep="finishWorkspaceSwitch(false)"
        @sync="finishWorkspaceSwitch(true)"
      />
      <ConfirmDialog
        ref="confirmDialog"
        :kind="modalKind"
        :target-format="targetFormat"
        :filename="activeDocument?.filename"
        :date="selectedDate"
        :storage-mode="storageMode"
        :error="error"
        :busy="busy"
        @confirm="confirmModal"
        @cancel="cancelConfirm"
      />
      <SkinDialog ref="skinDialog" :active="activeSkin" @select="applySkin" @close="closeSkinDialog" />
      <LayoutDialog
        ref="layoutDialog"
        :saved-layouts="savedLayouts"
        @reset="resetLayout"
        @save="saveCurrentLayout"
        @apply="applySavedLayout"
        @remove="removeSavedLayout"
        @close="closeLayoutDialog"
      />
      <JournalPreviewWindow
        :visible="journalPreview.visible.value && page === 'home' && !!workspace"
        :top-inset="nativeApp && isMac ? 36 : 0"
        :anchor-rect="editorPreviewAnchor"
        @close="journalPreview.close"
      >
        <JournalPreviewContent />
      </JournalPreviewWindow>
    </div>
    <AppRail
      v-if="workspace"
      :disabled="busy || excelBusy || picking"
      :active="layoutItems.map((item) => item.id)"
      :settings-active="page === 'settings'"
      :settings-busy="settingsLoading || settingsSaving || settingsLeaveBusy || databaseBusy"
      @module="activateModule"
      @refresh="refreshHome"
      @layout="openLayoutFromRail"
      @settings="toggleSettings"
      @skin="openSkinDialog"
    />
    <MessageHost />
  </div>
</template>
