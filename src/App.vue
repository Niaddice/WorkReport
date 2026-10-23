<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { open as pickDirectory } from '@tauri-apps/plugin-dialog'
import { PhFolderOpen, PhArrowRight, PhArrowClockwise, PhGearSix, PhHouse, PhX, PhFolder, PhFileText } from '@phosphor-icons/vue'
import CalendarPane from './components/CalendarPane.vue'
import JournalEditor from './components/JournalEditor.vue'
import SettingsView from './components/SettingsView.vue'
import AiPolishDialog from './components/AiPolishDialog.vue'
import AiSummaryDialog from './components/AiSummaryDialog.vue'
import RangeOverview from './components/RangeOverview.vue'
import { api, type Document, type FileFormat } from './lib/workspace'
import { historyShortcut } from './lib/shortcuts'
import { useJournal } from './lib/useJournal'
import { aiSummarySource, plainTextSummary } from './lib/summary'
import { useAi } from './lib/ai'
import { createDialogMotion } from './lib/dialogMotion'
import { useAppearance } from './lib/useAppearance'
import { sameWeekRange, useExcel } from './lib/useExcel'
import { useSummaryOutput } from './lib/useSummaryOutput'
import { settingsApi, type WorkspaceSettings } from './lib/settingsDraft'
import { databaseApi, cloneDatabaseSettings, DEFAULT_DATABASE_SETTINGS, type DatabaseTransferResult } from './lib/database'
import { dateKey } from './lib/calendar'
import { useHolidays } from './lib/holidays'
import { useHolidayReminder } from './lib/useHolidayReminder'

const journal = useJournal()
const { workspace, entries, previews, selectedDate, document: activeDocument, isNewDraft, draft, dirty, canUndo, canRedo, busy, saving, error, backupWarning, backupRetrying, savedAt, loadPreviews } = journal
const ai = useAi()
const appearance = useAppearance()
const excel = useExcel()
const summaryOutput = useSummaryOutput()
const holidays = useHolidays()
const { state: holidayState, loading: holidayLoading, syncing: holidaySyncing, error: holidayError } = holidays
const holidayReady = ref(false)
const { settings: summaryOutputSettings, loading: summaryOutputLoading, saving: summaryOutputSaving, error: summaryOutputError, exporting: summaryExporting, opening: summaryDirectoryOpening, result: summaryExportResult, exportError: summaryExportError, openError: summaryDirectoryError } = summaryOutput
const summaryWorkspacePath = ref('')
const summaryDirectory = computed(() => {
  const root = summaryWorkspacePath.value.replace(/[\\/]+$/, '')
  if (!root) return ''
  const separator = root.includes('\\') ? '\\' : '/'
  return `${root}${separator}${summaryOutputSettings.value.outputDirectory.replace(/[\\/]/g, separator)}`
})
const { settings: excelSettings, loading: excelLoading, saving: excelSaving, error: excelSettingsError, preparing: excelPreparing, exporting: excelExporting, preview: excelPreview, result: excelResult, exportError: excelExportError } = excel
const { scale: appearanceScale, loading: appearanceLoading, saving: appearanceSaving, error: appearanceError } = appearance
watch(appearanceScale, (scale) => {
  document.documentElement.style.setProperty('--font-scale', String(scale / 100))
}, { immediate: true, flush: 'sync' })
// Settings live inside the workspace, so a newly opened workspace brings its own values.
watch(workspace, (value, previous) => {
  if (!value || value.path === previous?.path) return
  void loadWorkspaceSettings(true)
})
const { settings: aiSettings, error: aiSettingsError, saving: aiSaving, defaultProvider, visible: aiVisible, generating, elapsedSeconds, original: aiOriginal, result: aiResult, resultError: aiResultError, summaryVisible, summaryGenerating, summaryElapsedSeconds, summaryDates, summaryResult, summaryError, summaryMode } = ai
watch(summaryResult, () => summaryOutput.clearExport())
const page = ref<'home' | 'settings'>('home')
const recent = ref<string | null>(null)
const picking = ref(false)
const openingDirectory = ref(false)
const requestingAi = ref(false)
const requestingExcel = ref(false)
const excelBusy = computed(() => requestingExcel.value || excelPreparing.value || excelExporting.value)
const summarySelectionDates = ref<string[]>([])
const excelDirectoryError = ref('')
let excelActionRevision = 0
let summarySessionRevision = 0
watch(summaryVisible, () => {
  excelActionRevision++
  requestingExcel.value = false
  excelDirectoryError.value = ''
  excel.clearExport()
}, { flush: 'sync' })
watch(summaryVisible, () => { summarySessionRevision++ }, { flush: 'sync' })
const refreshRevision = ref(0)
const settingsView = ref<InstanceType<typeof SettingsView>>()
const databaseSettings = ref(cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS))
const databaseLoading = ref(false)
const settingsDatabaseError = ref('')
const databaseActivity = ref(false)
const storageRefreshing = ref(false)
const databaseBusy = computed(() => databaseActivity.value || storageRefreshing.value || backupRetrying.value)
const storageMode = computed(() => !databaseSettings.value.enabled ? 'files' : databaseSettings.value.mode)

const workspaceSettings = computed<WorkspaceSettings>(() => ({ appearance: { scale: appearanceScale.value }, ai: aiSettings.value, excel: excelSettings.value, summary: summaryOutputSettings.value, database: databaseSettings.value }))
const settingsReading = ref(false)
const settingsSaving = ref(false)
const settingsReady = ref(false)
const settingsError = ref('')
const settingsLoading = computed(() => settingsReading.value || appearanceLoading.value || excelLoading.value || summaryOutputLoading.value || databaseLoading.value)
let settingsLoadRevision = 0
type SettingsExit = 'home' | 'workspace' | 'close'
const settingsLeaveDialog = ref<HTMLDialogElement>()
const settingsLeaveMotion = createDialogMotion(() => settingsLeaveDialog.value)
const pendingSettingsExit = ref<SettingsExit | null>(null)
const settingsLeaveBusy = ref(false)
const settingsLeaveError = ref('')
const modal = ref<HTMLDialogElement>()
const modalMotion = createDialogMotion(() => modal.value)
const modalKind = ref<'reload' | 'format' | 'restore'>('format')
const targetFormat = ref<FileFormat>('md')
const editor = ref<InstanceType<typeof JournalEditor>>()
const editorPreview = ref(false)
const polishDialog = ref<InstanceType<typeof AiPolishDialog>>()
const calendar = ref<InstanceType<typeof CalendarPane>>()
const layoutEl = ref<HTMLElement>()
const calendarWidth = ref<number | null>(null)
const resizing = ref(false)
const rangeDates = ref<string[]>([])
const rangeDocuments = ref<Document[]>([])
const rangeLoading = ref(false)
const rangeError = ref('')
const excelBlockedReason = computed(() => {
  if (!sameWeekRange(summarySelectionDates.value)) return '仅支持同一周内的日期范围（周一至周日），包括没有日报的所选日期。'
  if (!defaultProvider.value) return '请先在设置中配置并选择默认 AI。'
  if (!excelSettings.value.templatePath || !excelSettings.value.prompt.trim() || !excelSettings.value.outputDirectory) return '请先完成 Excel 输出设置。'
  return ''
})
let rangeRevision = 0
const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent)
const now = ref(dayjs())
const today = computed(() => dateKey(now.value))
const holidayDate = computed(() => now.value.format('YYYY-MM-DD'))
const { visible: holidayReminderVisible, dismiss: dismissHolidayReminder } = useHolidayReminder(holidayState, holidayDate, holidayReady)
const holidayCurrentFailure = computed(() => {
  const record = holidayState.value.years.find(item => item.year === now.value.year())
  return holidayState.value.failures.some(item => item.year === now.value.year()
    && (!record || Date.parse(item.attemptedAt) >= Date.parse(record.syncedAt)))
})
const nativeApp = isTauri()
const recentName = computed(() => recent.value?.split(/[\\/]/).filter(Boolean).at(-1))
const greeting = computed(() => now.value.hour() < 12 ? '上午好' : now.value.hour() < 18 ? '下午好' : '晚上好')
const documentKey = computed(() => `${workspace.value?.path ?? ''}/${selectedDate.value}`)
const backups = ref<Record<string, string>>({})
const canRestore = computed(() => backups.value[documentKey.value] !== undefined)
let aiDocumentKey = ''
let unlistenClose: (() => void) | undefined
let unlistenHistory: (() => void) | undefined
let historyMenuAvailable = false
let clockTimer: ReturnType<typeof setInterval> | undefined
const CALENDAR_MIN_WIDTH = 420
const EDITOR_MIN_WIDTH = 340
let layoutObserver: ResizeObserver | undefined
let splitterStartX = 0
let splitterStartWidth = 0
const layoutStyle = computed(() => calendarWidth.value === null ? undefined : { '--calendar-width': `${calendarWidth.value}px` })

async function navigate(target: 'home' | 'settings') {
  if (busy.value || picking.value || settingsSaving.value || settingsReading.value || databaseBusy.value || settingsLeaveBusy.value || aiVisible.value || summaryExporting.value || excelBusy.value || page.value === target) return
  if (target === 'home' && requestSettingsExit('home')) return
  if (!(await journal.flush())) return
  page.value = target
  if (target === 'settings') {
    await loadWorkspaceSettings(false)
    await nextTick()
    settingsView.value?.reset()
  }
}
async function openHolidaySettings() {
  await navigate('settings')
  if (page.value === 'settings') await settingsView.value?.navigate('holidays')
}
async function loadWorkspaceSettings(reload: boolean): Promise<boolean> {
  const revision = ++settingsLoadRevision
  settingsReading.value = true; settingsReady.value = false; settingsError.value = ''
  const results = await Promise.all([
    reload ? appearance.reload() : appearance.load(), reload ? ai.reload() : ai.load(),
    reload ? excel.reload() : excel.load(), reload ? summaryOutput.reload() : summaryOutput.load(), loadDatabaseSettings(),
  ])
  if (revision !== settingsLoadRevision) return false
  settingsReading.value = false
  settingsReady.value = results.every(Boolean)
  if (!settingsReady.value) settingsError.value = [appearanceError.value, aiSettingsError.value, excelSettingsError.value, summaryOutputError.value, settingsDatabaseError.value].filter(Boolean).join('；') || '读取设置失败，请重新进入设置后重试。'
  return settingsReady.value
}
async function loadDatabaseSettings(): Promise<boolean> {
  const path = workspace.value?.path
  databaseLoading.value = true; settingsDatabaseError.value = ''
  try {
    const stored = nativeApp ? await databaseApi.load() : cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS)
    if (workspace.value?.path !== path) return false
    databaseSettings.value = cloneDatabaseSettings(stored)
    return true
  } catch (cause) {
    if (workspace.value?.path === path) settingsDatabaseError.value = `读取数据库设置失败：${String(cause)}`
    return false
  } finally { if (workspace.value?.path === path) databaseLoading.value = false }
}
async function onDatabaseTransferred(result: DatabaseTransferResult) {
  storageRefreshing.value = true
  try {
    databaseSettings.value = cloneDatabaseSettings(result.settings)
    await nextTick()
    settingsView.value?.reset()
    clearRange(); calendar.value?.clearRange()
    await journal.reload()
  } finally { storageRefreshing.value = false }
}
async function saveWorkspaceSettings(value: WorkspaceSettings): Promise<boolean> {
  if (settingsSaving.value || databaseBusy.value) return false
  if (!workspace.value || !settingsReady.value) { settingsError.value = '请先打开工作空间并完整读取设置后重试。'; return false }
  const path = workspace.value.path
  settingsSaving.value = true; settingsError.value = ''
  try {
    if (!(await journal.flush())) { settingsError.value = '请先保存当前日报，再更改存储设置。'; return false }
    const storageChanged = JSON.stringify(value.database) !== JSON.stringify(databaseSettings.value)
    const stored = await settingsApi.save(path, value)
    if (workspace.value?.path !== path) return false
    ai.acceptSaved(stored.ai)
    appearance.acceptSaved(stored.appearance)
    excel.acceptSaved(stored.excel)
    summaryOutput.acceptSaved(stored.summary)
    databaseSettings.value = cloneDatabaseSettings(stored.database)
    if (storageChanged) {
      clearRange(); calendar.value?.clearRange()
      await journal.reload()
    }
    return true
  } catch (cause) { settingsError.value = `保存设置失败：${String(cause)}`; return false }
  finally { settingsSaving.value = false }
}
function requestSettingsExit(action: SettingsExit): boolean {
  if (page.value !== 'settings' || !settingsView.value || settingsView.value.unchanged) return false
  if (!pendingSettingsExit.value) {
    pendingSettingsExit.value = action
    settingsLeaveError.value = ''
    settingsLeaveMotion.open()
  }
  return true
}
function cancelSettingsExit() {
  if (settingsLeaveBusy.value || settingsSaving.value) return
  pendingSettingsExit.value = null; settingsLeaveError.value = ''
  void settingsLeaveMotion.close()
}
async function finishSettingsExit(save: boolean) {
  if (!pendingSettingsExit.value || settingsLeaveBusy.value || settingsSaving.value) return
  settingsLeaveBusy.value = true; settingsLeaveError.value = ''
  const action = pendingSettingsExit.value
  try {
    if (save && !(await settingsView.value?.save())) {
      settingsLeaveError.value = settingsError.value || '设置未保存，请继续编辑并检查标出的内容。'
      return
    }
    if (!save) settingsView.value?.reset()
    pendingSettingsExit.value = null
    if (!(await settingsLeaveMotion.close())) return
    if (action === 'workspace') await performChooseWorkspace()
    else if (action === 'close') await closeWindow()
    else if (await journal.flush()) page.value = 'home'
  } finally { settingsLeaveBusy.value = false }
}
async function closeWindow() {
  if (excelExporting.value) return
  rangeRevision++; ai.close(); ai.closeSummary(); await excel.cancel()
  if (await journal.flush()) await getCurrentWindow().destroy()
}
async function openWorkspaceDirectory() {
  if (!workspace.value || openingDirectory.value) return
  openingDirectory.value = true
  try { await api.openDirectory() }
  catch (cause) { journal.reportError(cause) }
  finally { openingDirectory.value = false }
}
async function chooseWorkspace() {
  if (picking.value || busy.value || settingsSaving.value || settingsReading.value || databaseBusy.value || settingsLeaveBusy.value || summaryExporting.value || excelBusy.value) return
  if (requestSettingsExit('workspace')) return
  await performChooseWorkspace()
}
async function performChooseWorkspace() {
  if (!nativeApp) { journal.reportError('请在 Tauri 桌面应用中选择本地文件夹。使用 npm run app:dev 启动。'); return }
  if (!(await journal.flush())) return
  picking.value = true
  try {
    const path = await pickDirectory({ directory: true, multiple: false, title: '选择 WorkReport 工作空间', defaultPath: workspace.value?.path ?? recent.value ?? undefined })
    if (typeof path === 'string' && await journal.open(path)) {
      calendar.value?.clearRange()
      clearRange()
      recent.value = workspace.value?.path ?? path
    }
  } catch (cause) { journal.reportError(cause) }
  finally { picking.value = false }
}
function showConfirmation() {
  modalMotion.open()
}
async function selectDate(date: string) {
  if (excelBusy.value) return
  if (await journal.select(date)) editor.value?.focus()
}
async function requestFormat(format: FileFormat) {
  if (isNewDraft.value) {
    if (await journal.switchFormat(format)) editor.value?.focus()
    return
  }
  targetFormat.value = format; modalKind.value = 'format'; showConfirmation()
}
function requestReload() { modalKind.value = 'reload'; showConfirmation() }
function requestRestore() { modalKind.value = 'restore'; showConfirmation() }
async function confirmModal() {
  if (modalKind.value === 'format') {
    if (await journal.switchFormat(targetFormat.value) && await modalMotion.close()) editor.value?.focus()
  } else if (modalKind.value === 'restore') {
    const original = backups.value[documentKey.value]
    if (original !== undefined) { journal.edit(original); if (!(await journal.flush())) return; delete backups.value[documentKey.value] }
    modalMotion.close()
  } else { await journal.reload(); modalMotion.close() }
}
async function requestAi() {
  if (requestingAi.value || !activeDocument.value || busy.value || summaryVisible.value || excelBusy.value) return
  const sourceKey = documentKey.value
  requestingAi.value = true
  try {
    if (!(await journal.flush())) return
    if (documentKey.value !== sourceKey || page.value !== 'home') return
    const loaded = await ai.load()
    if (documentKey.value !== sourceKey || page.value !== 'home') return
    if (!loaded) { journal.reportError(`无法读取 AI 设置：${aiSettingsError.value}`); return }
    if (!defaultProvider.value) { await navigate('settings'); return }
    if (!draft.value.trim()) { journal.reportError('请先写下日报内容，再使用 AI 润色。'); return }
    if (!(await journal.flush())) return
    if (documentKey.value !== sourceKey || page.value !== 'home' || !activeDocument.value) return
    aiDocumentKey = documentKey.value
    ai.open(draft.value, activeDocument.value.format)
  } finally { requestingAi.value = false }
}
function clearRange() {
  excel.clearExport()
  rangeRevision++
  rangeDates.value = []
  rangeDocuments.value = []
  rangeLoading.value = false
  rangeError.value = ''
}
function closeRangeView() {
  calendar.value?.clearRange()
  clearRange()
}
async function loadRange(dates: string[]) {
  if (excelBusy.value) return
  if (!dates.length) { clearRange(); return }
  const revision = ++rangeRevision
  excel.clearExport()
  rangeDates.value = [...dates].sort()
  rangeDocuments.value = []
  rangeError.value = ''
  rangeLoading.value = true
  try {
    if (!(await journal.flush()) && revision === rangeRevision) rangeError.value = '当前草稿保存失败，合并视图可能不是最新内容。'
    if (revision !== rangeRevision) return
    const selected = entries.value.filter((entry) => dates.includes(entry.date)).sort((left, right) => right.date.localeCompare(left.date))
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
  if (page.value !== 'home' || busy.value || aiVisible.value || summaryVisible.value || rangeDates.value.length || document.querySelector('dialog[open]')) return
  if (action === 'undo') journal.undo()
  else journal.redo()
}
function onHistoryKeydown(event: KeyboardEvent) {
  // On macOS the native Edit menu owns the key equivalents and forwards them
  // through the `history-action` event instead of dispatching keydown here.
  if (isMac && nativeApp && historyMenuAvailable) return
  if (page.value !== 'home' || busy.value || aiVisible.value || summaryVisible.value || rangeDates.value.length || document.querySelector('dialog[open]')) return
  const target = event.target
  if (target instanceof HTMLElement && target !== document.body && target.closest('input, textarea, [contenteditable="true"]') && !target.closest('.markdown-editor')) return
  const action = historyShortcut(event, isMac)
  if (!action) return
  event.preventDefault()
  performHistory(action)
}
async function requestSummary(dates: string[]) {
  if (requestingAi.value || busy.value || aiVisible.value || summaryVisible.value || excelBusy.value) return
  const workspacePath = workspace.value?.path
  const selectionRevision = rangeRevision
  requestingAi.value = true
  try {
    if (!(await journal.flush())) return
    if (workspace.value?.path !== workspacePath || page.value !== 'home' || selectionRevision !== rangeRevision) return
    const selected = entries.value.filter((entry) => dates.includes(entry.date)).sort((left, right) => left.date.localeCompare(right.date))
    if (!selected.length) { journal.reportError('所选日期还没有日报，无法生成总结。'); return }
    const documents = await Promise.all(selected.map((entry) => api.read(entry.date, entry.format)))
    if (workspace.value?.path !== workspacePath || page.value !== 'home' || selectionRevision !== rangeRevision) return
    if (!documents.some((document) => document.content.trim())) { journal.reportError('所选日报没有可总结的内容。'); return }
    const aiContent = aiSummarySource(documents)
    const directContent = plainTextSummary(documents)
    summaryWorkspacePath.value = workspacePath ?? ''
    summarySelectionDates.value = [...dates]
    summaryOutput.clearExport()
    ai.openSummary({ ai: aiContent, direct: directContent }, documents.map((document) => document.date))
  } catch (cause) { if (workspace.value?.path === workspacePath && page.value === 'home' && selectionRevision === rangeRevision) journal.reportError(cause) }
  finally { requestingAi.value = false }
}
function exportSummary(format: FileFormat) {
  if (!summaryVisible.value || requestingAi.value || summaryGenerating.value || summaryExporting.value || excelBusy.value || workspace.value?.path !== summaryWorkspacePath.value) return
  void summaryOutput.exportSummary(summaryWorkspacePath.value, [...summaryDates.value], summaryResult.value, format, summaryMode.value)
}
function openSummaryDirectory() {
  if (!summaryVisible.value || workspace.value?.path !== summaryWorkspacePath.value) return
  void summaryOutput.openDirectory(summaryWorkspacePath.value)
}
function reportExcelUnavailable() {
  if (!summaryVisible.value || summaryExporting.value || excelBusy.value) return
  excel.clearExport()
  excelDirectoryError.value = ''
  excelExportError.value = excelBlockedReason.value
}
async function requestExcelExport() {
  if (!summaryVisible.value || summaryExporting.value || excelBusy.value) return
  excel.clearExport()
  excelDirectoryError.value = ''
  const blockedReason = busy.value ? '工作空间正在处理中，请稍后重试。' : !excelSettings.value.enabled ? '请先在设置中启用 Excel 输出。' : excelBlockedReason.value
  if (blockedReason) { excelExportError.value = blockedReason; return }
  const dates = [...summarySelectionDates.value]
  const workspacePath = summaryWorkspacePath.value
  const revision = ++excelActionRevision
  requestingExcel.value = true
  try {
    if (!(await excel.flush())) return
    if (revision !== excelActionRevision || !summaryVisible.value || workspacePath !== workspace.value?.path) return
    await excel.prepareSelection(workspacePath, dates)
  } finally { if (revision === excelActionRevision) requestingExcel.value = false }
}
async function confirmExcelPreview() {
  if (!summaryVisible.value || summaryExporting.value || excelBusy.value || !excelPreview.value) return
  if (summaryWorkspacePath.value !== workspace.value?.path) { excelExportError.value = '工作空间已切换，请重新生成 Excel 预览。'; return }
  excelDirectoryError.value = ''
  await excel.confirmPreview(summaryWorkspacePath.value)
}
function discardExcelPreview() {
  if (excelExporting.value) return
  void excel.discardPreview()
}
async function openExcelOutputDirectory() {
  if (!nativeApp || !summaryVisible.value || summaryWorkspacePath.value !== workspace.value?.path) return
  try { await invoke<void>('open_excel_output_directory') }
  catch (cause) { excelDirectoryError.value = `无法打开 Excel 输出目录：${String(cause)}` }
}
async function cancelExcelExport() {
  if (excelExporting.value) return
  excelActionRevision++
  requestingExcel.value = false
  await excel.cancel()
}
async function startAiSummary() {
  if (requestingAi.value || summaryGenerating.value || summaryExporting.value || excelBusy.value || !summaryVisible.value) return
  const revision = summarySessionRevision
  requestingAi.value = true
  try {
    const loaded = await ai.load()
    if (revision !== summarySessionRevision || !summaryVisible.value) return
    if (!loaded) { ai.closeSummary(); journal.reportError(`无法读取 AI 设置：${aiSettingsError.value}`); return }
    if (!defaultProvider.value) { ai.closeSummary(); await navigate('settings'); return }
    void ai.generateSummary()
  } finally { requestingAi.value = false }
}
async function applyAi(content: string, restore = false) {
  if (documentKey.value !== aiDocumentKey) { ai.close(); journal.reportError('日报已切换，未应用润色结果。'); return }
  if (!restore) backups.value[documentKey.value] ??= aiOriginal.value
  journal.edit(content, true)
  ai.close()
  const closing = polishDialog.value?.close()
  await journal.flush()
  if (await closing && page.value === 'home' && !document.querySelector('dialog[open]') && documentKey.value === aiDocumentKey) editor.value?.focus()
}
function splitterPixelWidth() {
  return layoutEl.value?.querySelector<HTMLElement>('.journal-splitter')?.offsetWidth || 18
}
function calendarPanelWidth() {
  return layoutEl.value?.querySelector<HTMLElement>('.calendar-panel')?.offsetWidth ?? 0
}
function isStackedLayout() {
  const splitter = layoutEl.value?.querySelector<HTMLElement>('.journal-splitter')
  return !splitter || getComputedStyle(splitter).display === 'none'
}
function clampCalendarWidth(width: number) {
  const layout = layoutEl.value
  if (!layout || !layout.clientWidth) return width
  const max = Math.max(CALENDAR_MIN_WIDTH, layout.clientWidth - splitterPixelWidth() - EDITOR_MIN_WIDTH)
  return Math.min(Math.max(width, CALENDAR_MIN_WIDTH), max)
}
function setCalendarWidth(width: number) {
  if (!Number.isFinite(width)) return
  const next = Math.round(clampCalendarWidth(width))
  if (next !== calendarWidth.value) calendarWidth.value = next
}
function onSplitterMove(event: PointerEvent) {
  if (!resizing.value) return
  setCalendarWidth(splitterStartWidth + event.clientX - splitterStartX)
}
function stopSplitterResize() {
  resizing.value = false
  document.body.classList.remove('is-resizing')
  window.removeEventListener('pointermove', onSplitterMove)
  window.removeEventListener('pointerup', stopSplitterResize)
  window.removeEventListener('pointercancel', stopSplitterResize)
  window.removeEventListener('blur', stopSplitterResize)
}
function startSplitterResize(event: PointerEvent) {
  if (event.button !== 0 || !layoutEl.value || isStackedLayout()) return
  const width = calendarPanelWidth()
  if (!width) return
  event.preventDefault()
  resizing.value = true
  splitterStartX = event.clientX
  splitterStartWidth = width
  document.body.classList.add('is-resizing')
  window.addEventListener('pointermove', onSplitterMove)
  window.addEventListener('pointerup', stopSplitterResize)
  window.addEventListener('pointercancel', stopSplitterResize)
  window.addEventListener('blur', stopSplitterResize)
}
function onSplitterKeydown(event: KeyboardEvent) {
  if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
  const width = calendarPanelWidth()
  if (!width) return
  event.preventDefault()
  const step = event.shiftKey ? 48 : 16
  setCalendarWidth(width + (event.key === 'ArrowLeft' ? -step : step))
}
function resetSplitterWidth() { calendarWidth.value = null }
function beforeUnload(event: BeforeUnloadEvent) { if (dirty.value || saving.value || settingsSaving.value || databaseBusy.value || aiSaving.value || appearanceSaving.value || excelSaving.value || summaryOutputSaving.value || summaryExporting.value || excelBusy.value || (page.value === 'settings' && !settingsView.value?.unchanged)) { event.preventDefault(); event.returnValue = '' } }
function onFocus() {
  now.value = dayjs()
  if (!dirty.value && !saving.value && !error.value && !modal.value?.open && !picking.value && !aiVisible.value && !summaryVisible.value && !excelBusy.value && !rangeDates.value.length && !databaseBusy.value) void journal.refresh()
}
watch(layoutEl, (element) => {
  layoutObserver?.disconnect()
  if (!element || typeof ResizeObserver === 'undefined') return
  layoutObserver = new ResizeObserver(() => {
    if (calendarWidth.value === null || isStackedLayout()) return
    setCalendarWidth(calendarWidth.value)
  })
  layoutObserver.observe(element)
}, { flush: 'post' })
onMounted(async () => {
  window.addEventListener('beforeunload', beforeUnload); window.addEventListener('focus', onFocus); window.addEventListener('keydown', onHistoryKeydown)
  clockTimer = setInterval(() => { now.value = dayjs() }, 60_000)
  void appearance.load()
  void excel.load()
  void holidays.load().finally(() => { holidayReady.value = true })
  if (nativeApp) {
    void ai.load()
    try {
      recent.value = await api.recent()
      unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => { event.preventDefault(); if (busy.value || picking.value || settingsSaving.value || settingsReading.value || databaseBusy.value || settingsLeaveBusy.value || summaryExporting.value) return; if (requestSettingsExit('close')) return; await closeWindow() })
      unlistenHistory = await listen<string>('history-action', (event) => {
        if (event.payload === 'undo' || event.payload === 'redo') performHistory(event.payload)
      })
      historyMenuAvailable = true
    } catch (cause) { journal.reportError(cause) }
  }
})
onBeforeUnmount(() => { rangeRevision++; settingsLoadRevision++; stopSplitterResize(); layoutObserver?.disconnect(); window.removeEventListener('beforeunload', beforeUnload); window.removeEventListener('focus', onFocus); window.removeEventListener('keydown', onHistoryKeydown); clearInterval(clockTimer); unlistenClose?.(); unlistenHistory?.(); ai.close(); ai.closeSummary(); void excel.cancel(); modalMotion.dispose(); settingsLeaveMotion.dispose(); journal.dispose() })
</script>

<template>
  <div class="app-shell">
    <header class="home-heading app-toolbar">
      <h1 class="home-greeting" :title="`${greeting}，今天是 ${now.format('M 月 D 日 dddd')}`"><span class="greeting-salutation">{{ greeting }}</span><span class="greeting-date">，今天是 {{ now.format('M 月 D 日 dddd') }}</span></h1>
      <div class="home-heading-actions">
        <Transition name="toolbar-switch" mode="out-in">
        <div :key="page" class="toolbar-action-group">
        <button v-if="page === 'settings'" type="button" class="toolbar-icon-button" aria-label="返回首页" title="返回首页" :disabled="busy || excelBusy || settingsSaving || settingsReading || settingsLeaveBusy || databaseBusy" @click="navigate('home')"><PhHouse :size="20" aria-hidden="true" /></button>
        <template v-else>
          <button type="button" class="toolbar-icon-button" aria-label="刷新" title="刷新" :disabled="!workspace || busy || picking || excelBusy" @click="refreshHome"><PhArrowClockwise :key="refreshRevision" :class="{ 'refresh-feedback': refreshRevision > 0 }" :size="20" aria-hidden="true" /></button>
          <button type="button" class="toolbar-icon-button" aria-label="设置" title="设置" :disabled="busy || excelBusy" @click="navigate('settings')"><PhGearSix :size="20" aria-hidden="true" /></button>
        </template>
        </div>
        </Transition>
      </div>
    </header>
    <div class="app-content">
    <Transition name="status-fade">
      <div v-if="holidayReminderVisible" class="holiday-reminder" role="status">
        <span>{{ now.year() }} 年放假信息{{ holidayCurrentFailure ? '上次同步失败' : '尚未同步成功' }}，请手动同步到日历。</span>
        <div class="holiday-reminder-actions"><button type="button" class="text-action" :disabled="busy || picking || settingsSaving || settingsReading || databaseBusy || settingsLeaveBusy || aiVisible || summaryVisible || summaryExporting || excelBusy" @click="openHolidaySettings">前往同步</button><button type="button" class="toolbar-icon-button" aria-label="关闭放假同步提醒" title="关闭提醒" @click="dismissHolidayReminder"><PhX :size="16" aria-hidden="true" /></button></div>
      </div>
    </Transition>
    <Transition name="page-switch">
    <div v-show="page === 'settings'" class="settings-viewport" :inert="page !== 'settings'">
      <Transition name="status-fade"><div v-if="error" class="settings-workspace-error error-banner" role="alert">{{ error }}<button class="text-action" @click="error = ''">关闭</button></div></Transition>
      <SettingsView :backup-warning="backupWarning" :backup-retrying="backupRetrying" @retry-backup="journal.retryBackup" :key="workspace?.path ?? 'no-workspace'" ref="settingsView" :workspace="workspace" :picking="picking || busy || excelBusy || settingsSaving" :bundle="workspaceSettings" :loading="settingsLoading" :saving="settingsSaving" :database-busy="databaseBusy" :before-database-transfer="journal.flush" @database-busy="databaseActivity = $event" @database-transferred="onDatabaseTransferred" :error="settingsError" :save="saveWorkspaceSettings" :holidays="holidayState" :holiday-loading="holidayLoading" :holiday-syncing="holidaySyncing" :holiday-error="holidayError" :current-year="now.year()" @sync-holidays="holidays.sync($event)" @back="navigate('home')" @workspace="chooseWorkspace" />
    </div>
    </Transition>
    <Transition name="page-switch">
    <main v-if="workspace" v-show="page === 'home'" class="home" :inert="page !== 'home'">
      <Transition name="status-fade"><div v-if="backupWarning" class="error-banner database-backup-warning" role="status"><span>{{ backupWarning }}</span><md-text-button :disabled="backupRetrying || busy || databaseBusy" @click="journal.retryBackup">{{ backupRetrying ? '正在重试…' : '重试数据库备份' }}</md-text-button></div></Transition>
      <Transition name="status-fade"><div v-if="error" class="error-banner" role="alert"><span>{{ error }}</span><md-text-button v-if="dirty" :disabled="saving" @click="journal.flush">重试保存</md-text-button><md-icon-button v-else aria-label="关闭提示" @click="error = ''"><PhX :size="18" /></md-icon-button></div></Transition>
      <div ref="layoutEl" class="journal-layout" :style="layoutStyle"><CalendarPane ref="calendar" :today="today" :selected="selectedDate" :entries="entries" :previews="previews" :disabled="busy || aiVisible || summaryVisible || requestingAi || excelBusy" @select="selectDate" @previews="loadPreviews" @range="loadRange" /><div class="journal-splitter" :class="{ 'is-dragging': resizing }" role="separator" aria-orientation="vertical" aria-label="调整日历和编辑区宽度" tabindex="0" title="拖动调整宽度，双击恢复默认" @pointerdown="startSplitterResize" @keydown="onSplitterKeydown" @dblclick="resetSplitterWidth"></div><div class="editor-stage"><Transition name="editor-switch"><RangeOverview v-if="rangeDates.length" key="range" v-model:preview="editorPreview" :dates="rangeDates" :documents="rangeDocuments" :loading="rangeLoading" :error="rangeError" :summarizing="summaryVisible" @close="closeRangeView" @summarize="requestSummary(rangeDates)" /><JournalEditor v-else key="editor" v-model:preview="editorPreview" ref="editor" :date="selectedDate" :document="activeDocument" :is-new="isNewDraft" :content="draft" :dirty="dirty" :saving="saving" :disabled="busy || aiVisible || summaryVisible || requestingAi || excelBusy" :error="error" :saved-at="savedAt" :storage-mode="storageMode" :can-restore="canRestore" :can-undo="canUndo" :can-redo="canRedo" @edit="journal.edit" @undo="journal.undo" @redo="journal.redo" @save="journal.flush" @reload="requestReload" @format="requestFormat" @ai="requestAi" @restore="requestRestore" @error="journal.reportError" /></Transition></div></div>
      <footer class="home-footer"><span :title="workspace.path"><button class="workspace-folder-button" aria-label="打开工作空间目录" title="打开工作空间目录" :disabled="openingDirectory" @click="openWorkspaceDirectory"><PhFolder :size="15" /></button><span class="workspace-path-text">{{ workspace.path }}</span></span><span>每一天，一份工作记录</span></footer>
    </main>
    </Transition>
    <Transition name="page-switch">
    <main v-if="!workspace && page === 'home'" class="workspace-gate" :inert="page !== 'home'"><div class="welcome-art" aria-hidden="true"><div class="art-folder"><PhFolderOpen :size="68" weight="light" /></div><span class="art-file"><PhFileText :size="30" weight="light" /></span></div><span class="welcome-label">你的工作，从这里开始</span><h1>为日报，选一个家。</h1><p>选择一个文件夹作为工作空间。<br />每天的记录，保存为 Markdown 或 TXT 文件。</p><md-filled-button :disabled="picking || busy" @click="chooseWorkspace"><PhFolderOpen slot="icon" :size="19" />{{ picking || busy ? '正在打开…' : '选择工作空间' }}</md-filled-button><button v-if="recent" class="recent-workspace" :disabled="picking || busy" :title="recent" @click="journal.open(recent)"><PhFolder :size="18" /><span><small>继续使用上次的工作空间</small><strong>{{ recentName }}</strong></span><PhArrowRight :size="18" /></button><p v-if="error" class="gate-error" role="alert">{{ error }}</p><div class="welcome-footnote">文件保存在本地，随时可以用其他编辑器打开。</div></main>
    </Transition>
    </div>
    <dialog ref="settingsLeaveDialog" tabindex="-1" role="alertdialog" aria-modal="true" class="confirmation-dialog settings-leave-dialog" aria-labelledby="settings-leave-title" aria-describedby="settings-leave-description" @cancel.prevent="cancelSettingsExit">
      <div class="dialog-icon"><PhGearSix :size="28" weight="light" /></div><h2 id="settings-leave-title">保存设置修改？</h2>
      <p id="settings-leave-description">设置中有未保存的修改。保存后再{{ pendingSettingsExit === 'close' ? '关闭窗口' : pendingSettingsExit === 'workspace' ? '切换工作空间' : '返回首页' }}，或放弃本次修改。</p>
      <p v-if="settingsLeaveError" class="error-text dialog-error" role="alert">{{ settingsLeaveError }}</p>
      <div class="dialog-actions"><md-text-button class="dialog-button dialog-button-secondary" :disabled="settingsLeaveBusy || settingsSaving" @click="cancelSettingsExit">继续编辑</md-text-button><md-text-button class="dialog-button dialog-button-secondary" :disabled="settingsLeaveBusy || settingsSaving" @click="finishSettingsExit(false)">不保存</md-text-button><md-filled-button class="dialog-button dialog-button-primary" :disabled="settingsLeaveBusy || settingsSaving" @click="finishSettingsExit(true)">{{ settingsLeaveBusy || settingsSaving ? '正在保存…' : '保存并继续' }}</md-filled-button></div>
    </dialog>
    <dialog ref="modal" tabindex="-1" role="dialog" aria-modal="true" class="confirmation-dialog" aria-labelledby="dialog-title" aria-describedby="dialog-description" @cancel.prevent="!busy && modalMotion.close()"><div class="dialog-icon"><PhFileText :size="28" weight="light" /></div><h2 id="dialog-title">{{ modalKind === 'format' ? '切换日报文件格式？' : modalKind === 'restore' ? '恢复润色前的原文？' : '重新载入磁盘内容？' }}</h2>
      <p id="dialog-description" v-if="modalKind === 'format'">将 <strong>{{ activeDocument?.filename }}</strong> 切换为 <strong>{{ selectedDate }}.{{ targetFormat }}</strong>。<br /><template v-if="storageMode === 'database'">文本内容保持原样，仅更新数据库中的日报格式，已有文件保持不变。</template><template v-else>文本内容保持原样，保存后会更换文件扩展名。若目标文件已存在，将阻止切换。</template></p><p id="dialog-description" v-else-if="modalKind === 'restore'">将使用 AI 润色前保留的原始日报替换当前文本，并自动保存。</p><p id="dialog-description" v-else>当前未保存的编辑将被{{ storageMode === 'database' ? '数据库中的内容' : '磁盘文件' }}替换。<br />请先复制需要保留的草稿。</p><p v-if="error" class="error-text dialog-error" role="alert">{{ error }}</p><div class="dialog-actions"><md-text-button class="dialog-button dialog-button-secondary" :disabled="busy" @click="modalMotion.close()">取消</md-text-button><md-filled-button class="dialog-button dialog-button-primary" :disabled="busy" @click="confirmModal">{{ busy ? '处理中…' : modalKind === 'format' ? '确认切换' : modalKind === 'restore' ? '恢复原文' : '重新载入' }}</md-filled-button></div></dialog>
    <AiSummaryDialog :visible="summaryVisible" :generating="summaryGenerating || requestingAi" :elapsed-seconds="summaryElapsedSeconds" :dates="summaryDates" :result="summaryResult" :error="summaryError" :provider="defaultProvider?.name ?? ''" :mode="summaryMode" :output-directory="summaryDirectory" :output-loading="summaryOutputLoading" :output-exporting="summaryExporting" :directory-opening="summaryDirectoryOpening" :output-error="summaryExportError || summaryDirectoryError || summaryOutputError" :exported-file="summaryExportResult?.filename ?? ''" :excel-enabled="excelSettings.enabled" :excel-preparing="excelPreparing || requestingExcel" :excel-exporting="excelExporting" :excel-preview="excelPreview" :excel-blocked-reason="excelBlockedReason" :excel-result="excelResult" :excel-error="excelExportError || excelDirectoryError || excelSettingsError" @excel="requestExcelExport" @excel-unavailable="reportExcelUnavailable" @cancel-excel="cancelExcelExport" @confirm-excel="confirmExcelPreview" @discard-excel="discardExcelPreview" @regenerate-excel="requestExcelExport" @open-excel-directory="openExcelOutputDirectory" @export="exportSummary" @open-directory="openSummaryDirectory" @close="ai.closeSummary" @ai="startAiSummary" @direct="ai.directSummary" />
    <AiPolishDialog ref="polishDialog" :visible="aiVisible" :generating="generating" :elapsed-seconds="elapsedSeconds" :original="aiOriginal" :result="aiResult" :error="aiResultError" :provider="defaultProvider?.name ?? ''" :provider-kind="defaultProvider?.kind ?? ''" @close="ai.close" @regenerate="ai.generate" @edit="aiResult = $event; aiResultError = ''" @apply="applyAi(aiResult)" @restore="applyAi(aiOriginal, true)" />
  </div>
</template>
