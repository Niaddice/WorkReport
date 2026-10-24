<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { open as pickDirectory } from '@tauri-apps/plugin-dialog'
import { PhFolder, PhX } from '@phosphor-icons/vue'
import AppToolbar from './components/AppToolbar.vue'
import AppRail from './components/AppRail.vue'
import SkinDialog from './components/SkinDialog.vue'
import CalendarPane from './components/CalendarPane.vue'
import JournalEditor from './components/JournalEditor.vue'
import SettingsView from './components/SettingsView.vue'
import AiPolishDialog from './components/AiPolishDialog.vue'
import AiSummaryDialog from './components/AiSummaryDialog.vue'
import RangeOverview from './components/RangeOverview.vue'
import WorkspaceGate from './components/WorkspaceGate.vue'
import HolidayReminder from './components/HolidayReminder.vue'
import ConfirmDialog from './components/ConfirmDialog.vue'
import SettingsLeaveDialog from './components/SettingsLeaveDialog.vue'
import { api, type Document, type FileFormat } from './lib/workspace'
import { historyShortcut } from './lib/shortcuts'
import { useJournal } from './lib/useJournal'
import { aiSummarySource, plainTextSummary } from './lib/summary'
import { useAi } from './lib/ai'
import { useSplitter } from './lib/useSplitter'
import {
  DEFAULT_UI_PREFERENCES,
  normalizeUiPreferences,
  uiPreferencesApi,
  type LayoutModule,
  type UiPreferences,
} from './lib/uiPreferences'
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

const journal = useJournal()
const {
  workspace,
  entries,
  previews,
  selectedDate,
  document: activeDocument,
  isNewDraft,
  draft,
  dirty,
  canUndo,
  canRedo,
  busy,
  saving,
  error,
  backupWarning,
  backupRetrying,
  savedAt,
  loadPreviews,
} = journal
const ai = useAi()
const appearance = useAppearance()
const excel = useExcel()
const summaryOutput = useSummaryOutput()
const holidays = useHolidays()
const { state: holidayState, loading: holidayLoading, syncing: holidaySyncing, error: holidayError } = holidays
const holidayReady = ref(false)
const {
  settings: summaryOutputSettings,
  loading: summaryOutputLoading,
  error: summaryOutputError,
  exporting: summaryExporting,
  opening: summaryDirectoryOpening,
  result: summaryExportResult,
  exportError: summaryExportError,
  openError: summaryDirectoryError,
} = summaryOutput
const summaryWorkspacePath = ref('')
const summaryDirectory = computed(() => {
  const root = summaryWorkspacePath.value.replace(/[\\/]+$/, '')
  if (!root) return ''
  const separator = root.includes('\\') ? '\\' : '/'
  return `${root}${separator}${summaryOutputSettings.value.outputDirectory.replace(/[\\/]/g, separator)}`
})
const {
  settings: excelSettings,
  loading: excelLoading,
  error: excelSettingsError,
  preparing: excelPreparing,
  exporting: excelExporting,
  preview: excelPreview,
  result: excelResult,
  exportError: excelExportError,
} = excel
const { scale: appearanceScale, loading: appearanceLoading, error: appearanceError } = appearance
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
  visible: aiVisible,
  generating,
  elapsedSeconds,
  original: aiOriginal,
  result: aiResult,
  resultError: aiResultError,
  summaryVisible,
  summaryGenerating,
  summaryElapsedSeconds,
  summaryDates,
  summaryResult,
  summaryError,
  summaryMode,
} = ai
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
watch(
  summaryVisible,
  () => {
    excelActionRevision++
    requestingExcel.value = false
    excelDirectoryError.value = ''
    excel.clearExport()
  },
  { flush: 'sync' },
)
watch(
  summaryVisible,
  () => {
    summarySessionRevision++
  },
  { flush: 'sync' },
)
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
  appearance: { scale: appearanceScale.value },
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
    excelLoading.value ||
    summaryOutputLoading.value ||
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
const editor = ref<InstanceType<typeof JournalEditor>>()
const editorPreview = ref(false)
const polishDialog = ref<InstanceType<typeof AiPolishDialog>>()
const calendar = ref<InstanceType<typeof CalendarPane>>()
const layoutEl = ref<HTMLElement>()
const { resizing, layoutStyle, startSplitterResize, onSplitterKeydown, resetSplitterWidth } = useSplitter(layoutEl)
const uiPreferences = ref<UiPreferences>(normalizeUiPreferences(DEFAULT_UI_PREFERENCES))
const showSplitter = computed(() => uiPreferences.value.axis === 'row' && uiPreferences.value.order[0] === 'calendar')
function paneOrder(module: LayoutModule) {
  if (uiPreferences.value.axis === 'column') return uiPreferences.value.order.indexOf(module) + 1
  if (uiPreferences.value.order[0] === 'calendar') return module === 'calendar' ? 1 : 3
  return module === 'editor' ? 1 : 2
}
const layoutDragging = ref<LayoutModule | null>(null)
const layoutDropZone = ref<LayoutDropZone | null>(null)
const layoutZones = [
  { id: 'left', label: '移到左侧' },
  { id: 'right', label: '移到右侧' },
  { id: 'top', label: '移到上方' },
  { id: 'bottom', label: '移到下方' },
] as const
type LayoutDropZone = (typeof layoutZones)[number]['id']
const LAYOUT_LONG_PRESS_MS = 450
let layoutPressTimer: ReturnType<typeof setTimeout> | undefined
let layoutPressOrigin: { x: number; y: number } | null = null

watch(
  () => workspace.value?.path,
  (path) => {
    if (!path) {
      uiPreferences.value = normalizeUiPreferences(DEFAULT_UI_PREFERENCES)
      document.documentElement.dataset.skin = uiPreferences.value.skin
      return
    }
    void uiPreferencesApi
      .load()
      .then((preferences) => {
        uiPreferences.value = normalizeUiPreferences(preferences)
        document.documentElement.dataset.skin = uiPreferences.value.skin
      })
      .catch(() => {
        uiPreferences.value = normalizeUiPreferences(DEFAULT_UI_PREFERENCES)
        document.documentElement.dataset.skin = uiPreferences.value.skin
      })
  },
)

function cancelLayoutPress() {
  clearTimeout(layoutPressTimer)
  layoutPressTimer = undefined
  layoutPressOrigin = null
}
function endLayoutDrag() {
  layoutDragging.value = null
  layoutDropZone.value = null
  document.body.classList.remove('is-layout-dragging')
}
function layoutZoneAt(x: number, y: number): LayoutDropZone | null {
  const rect = layoutEl.value?.getBoundingClientRect()
  if (!rect || x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) return null
  const dx = (x - rect.left) / rect.width - 0.5
  const dy = (y - rect.top) / rect.height - 0.5
  if (Math.abs(dx) > Math.abs(dy)) return dx < 0 ? 'left' : 'right'
  return dy < 0 ? 'top' : 'bottom'
}
function onLayoutPointerDown(event: PointerEvent) {
  if (page.value !== 'home' || busy.value || event.button !== 0) return
  const target = event.target as HTMLElement | null
  if (!target || target.closest('button, a, input, textarea, select, [contenteditable="true"]')) return
  const pane = target.closest('.calendar-panel, .editor-panel')
  if (!pane || !target.closest('.calendar-toolbar, .editor-heading')) return
  const module: LayoutModule = pane.classList.contains('calendar-panel') ? 'calendar' : 'editor'
  layoutPressOrigin = { x: event.clientX, y: event.clientY }
  clearTimeout(layoutPressTimer)
  layoutPressTimer = setTimeout(() => {
    layoutPressTimer = undefined
    layoutDragging.value = module
    layoutDropZone.value = null
    document.body.classList.add('is-layout-dragging')
  }, LAYOUT_LONG_PRESS_MS)
}
function onLayoutPointerMove(event: PointerEvent) {
  if (layoutPressTimer && layoutPressOrigin) {
    const moved = Math.hypot(event.clientX - layoutPressOrigin.x, event.clientY - layoutPressOrigin.y)
    if (moved > 8) cancelLayoutPress()
  }
  if (layoutDragging.value) layoutDropZone.value = layoutZoneAt(event.clientX, event.clientY)
}
function applyLayoutDrop() {
  const module = layoutDragging.value
  const zone = layoutDropZone.value
  endLayoutDrag()
  if (!module || !zone) return
  const other: LayoutModule = module === 'calendar' ? 'editor' : 'calendar'
  const axis: UiPreferences['axis'] = zone === 'left' || zone === 'right' ? 'row' : 'column'
  const order: LayoutModule[] = zone === 'left' || zone === 'top' ? [module, other] : [other, module]
  if (axis === uiPreferences.value.axis && order[0] === uiPreferences.value.order[0]) return
  uiPreferences.value = { ...uiPreferences.value, axis, order }
  void uiPreferencesApi.save(uiPreferences.value).catch((cause) => {
    error.value = `保存界面布局失败：${String(cause)}`
  })
}
function onLayoutPointerUp() {
  if (layoutDragging.value) applyLayoutDrop()
  else cancelLayoutPress()
}
function onLayoutKeyDown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !layoutDragging.value) return
  cancelLayoutPress()
  endLayoutDrag()
}
function openSkinDialog() {
  skinDialog.value?.open()
}
function closeSkinDialog() {
  void skinDialog.value?.close()
}
function applySkin(id: SkinId) {
  uiPreferences.value = { ...uiPreferences.value, skin: id }
  document.documentElement.dataset.skin = id
  void uiPreferencesApi.save(uiPreferences.value).catch((cause) => {
    error.value = `保存皮肤失败：${String(cause)}`
  })
}
async function openSummaryFromRail() {
  await requestSummary([journal.selectedDate.value])
}
async function openExcelFromRail() {
  const base = dayjs(journal.selectedDate.value)
  const start = base.subtract((base.day() + 6) % 7, 'day')
  const dates = Array.from({ length: 7 }, (_, index) => start.add(index, 'day').format('YYYYMMDD'))
  await requestSummary(dates)
  if (summaryVisible.value) await requestExcelExport()
}
function openHistoryFromRail() {
  editor.value?.openHistory()
}
const rangeDates = ref<string[]>([])
const rangeDocuments = ref<Document[]>([])
const rangeLoading = ref(false)
const rangeError = ref('')
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
const holidayReminderDisabled = computed(
  () =>
    busy.value ||
    picking.value ||
    settingsSaving.value ||
    settingsReading.value ||
    databaseBusy.value ||
    settingsLeaveBusy.value ||
    aiVisible.value ||
    summaryVisible.value ||
    summaryExporting.value ||
    excelBusy.value,
)
const nativeApp = isTauri()
const recentName = computed(() => recent.value?.split(/[\\/]/).filter(Boolean).at(-1))
const greeting = computed(() => (now.value.hour() < 12 ? '上午好' : now.value.hour() < 18 ? '下午好' : '晚上好'))
const documentKey = computed(() => `${workspace.value?.path ?? ''}/${selectedDate.value}`)
const backups = ref<Record<string, string>>({})
const canRestore = computed(() => backups.value[documentKey.value] !== undefined)
let aiDocumentKey = ''
let unlistenClose: (() => void) | undefined
let unlistenHistory: (() => void) | undefined
let historyMenuAvailable = false
let clockTimer: ReturnType<typeof setInterval> | undefined

async function navigate(target: 'home' | 'settings') {
  if (
    busy.value ||
    picking.value ||
    settingsSaving.value ||
    settingsReading.value ||
    databaseBusy.value ||
    settingsLeaveBusy.value ||
    aiVisible.value ||
    summaryExporting.value ||
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
async function openHolidaySettings() {
  await navigate('settings')
  if (page.value === 'settings') await settingsView.value?.navigate('holidays')
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
        excelSettingsError.value,
        summaryOutputError.value,
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
  storageRefreshing.value = true
  try {
    databaseSettings.value = cloneDatabaseSettings(result.settings)
    await nextTick()
    settingsView.value?.reset()
    clearRange()
    calendar.value?.clearRange()
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
      calendar.value?.clearRange()
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
  ai.close()
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
    summaryExporting.value ||
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
    if (typeof path === 'string' && (await journal.open(path))) {
      calendar.value?.clearRange()
      clearRange()
      recent.value = workspace.value?.path ?? path
    }
  } catch (cause) {
    journal.reportError(cause)
  } finally {
    picking.value = false
  }
}
function showConfirmation() {
  confirmDialog.value?.open()
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
      editor.value?.focus()
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
function onAiResultEdit(value: string) {
  aiResult.value = value
  aiResultError.value = ''
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
    if (!loaded) {
      journal.reportError(`无法读取 AI 设置：${aiSettingsError.value}`)
      return
    }
    if (!defaultProvider.value) {
      await navigate('settings')
      return
    }
    if (!draft.value.trim()) {
      journal.reportError('请先写下日报内容，再使用 AI 润色。')
      return
    }
    if (!(await journal.flush())) return
    if (documentKey.value !== sourceKey || page.value !== 'home' || !activeDocument.value) return
    aiDocumentKey = documentKey.value
    ai.open(draft.value, activeDocument.value.format)
  } finally {
    requestingAi.value = false
  }
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
  if (
    page.value !== 'home' ||
    busy.value ||
    aiVisible.value ||
    summaryVisible.value ||
    rangeDates.value.length ||
    document.querySelector('dialog[open]')
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
    aiVisible.value ||
    summaryVisible.value ||
    rangeDates.value.length ||
    document.querySelector('dialog[open]')
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
  if (requestingAi.value || busy.value || aiVisible.value || summaryVisible.value || excelBusy.value) return
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
      journal.reportError('所选日期还没有日报，无法生成总结。')
      return
    }
    const documents = await Promise.all(selected.map((entry) => api.read(entry.date, entry.format)))
    if (workspace.value?.path !== workspacePath || page.value !== 'home' || selectionRevision !== rangeRevision) return
    if (!documents.some((document) => document.content.trim())) {
      journal.reportError('所选日报没有可总结的内容。')
      return
    }
    const aiContent = aiSummarySource(documents)
    const directContent = plainTextSummary(documents)
    summaryWorkspacePath.value = workspacePath ?? ''
    summarySelectionDates.value = [...dates]
    summaryOutput.clearExport()
    ai.openSummary(
      { ai: aiContent, direct: directContent },
      documents.map((document) => document.date),
    )
  } catch (cause) {
    if (workspace.value?.path === workspacePath && page.value === 'home' && selectionRevision === rangeRevision)
      journal.reportError(cause)
  } finally {
    requestingAi.value = false
  }
}
function exportSummary(format: FileFormat) {
  if (
    !summaryVisible.value ||
    requestingAi.value ||
    summaryGenerating.value ||
    summaryExporting.value ||
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
  const blockedReason = busy.value
    ? '工作空间正在处理中，请稍后重试。'
    : !excelSettings.value.enabled
      ? '请先在设置中启用 Excel 输出。'
      : excelBlockedReason.value
  if (blockedReason) {
    excelExportError.value = blockedReason
    return
  }
  const dates = [...summarySelectionDates.value]
  const workspacePath = summaryWorkspacePath.value
  const revision = ++excelActionRevision
  requestingExcel.value = true
  try {
    if (revision !== excelActionRevision || !summaryVisible.value || workspacePath !== workspace.value?.path) return
    await excel.prepareSelection(workspacePath, dates)
  } finally {
    if (revision === excelActionRevision) requestingExcel.value = false
  }
}
async function confirmExcelPreview() {
  if (!summaryVisible.value || summaryExporting.value || excelBusy.value || !excelPreview.value) return
  if (summaryWorkspacePath.value !== workspace.value?.path) {
    excelExportError.value = '工作空间已切换，请重新生成 Excel 预览。'
    return
  }
  excelDirectoryError.value = ''
  await excel.confirmPreview(summaryWorkspacePath.value)
}
function discardExcelPreview() {
  if (excelExporting.value) return
  void excel.discardPreview()
}
async function openExcelOutputDirectory() {
  if (!nativeApp || !summaryVisible.value || summaryWorkspacePath.value !== workspace.value?.path) return
  try {
    await invoke<void>('open_excel_output_directory')
  } catch (cause) {
    excelDirectoryError.value = `无法打开 Excel 输出目录：${String(cause)}`
  }
}
async function cancelExcelExport() {
  if (excelExporting.value) return
  excelActionRevision++
  requestingExcel.value = false
  await excel.cancel()
}
async function startAiSummary() {
  if (
    requestingAi.value ||
    summaryGenerating.value ||
    summaryExporting.value ||
    excelBusy.value ||
    !summaryVisible.value
  )
    return
  const revision = summarySessionRevision
  requestingAi.value = true
  try {
    const loaded = await ai.load()
    if (revision !== summarySessionRevision || !summaryVisible.value) return
    if (!loaded) {
      ai.closeSummary()
      journal.reportError(`无法读取 AI 设置：${aiSettingsError.value}`)
      return
    }
    if (!defaultProvider.value) {
      ai.closeSummary()
      await navigate('settings')
      return
    }
    void ai.generateSummary()
  } finally {
    requestingAi.value = false
  }
}
async function applyAi(content: string, restore = false) {
  if (documentKey.value !== aiDocumentKey) {
    ai.close()
    journal.reportError('日报已切换，未应用润色结果。')
    return
  }
  if (!restore) backups.value[documentKey.value] ??= aiOriginal.value
  journal.edit(content, true)
  ai.close()
  const closing = polishDialog.value?.close()
  await journal.flush()
  if (
    (await closing) &&
    page.value === 'home' &&
    !document.querySelector('dialog[open]') &&
    documentKey.value === aiDocumentKey
  )
    editor.value?.focus()
}
function beforeUnload(event: BeforeUnloadEvent) {
  if (
    dirty.value ||
    saving.value ||
    settingsSaving.value ||
    databaseBusy.value ||
    summaryExporting.value ||
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
    !aiVisible.value &&
    !summaryVisible.value &&
    !excelBusy.value &&
    !rangeDates.value.length &&
    !databaseBusy.value
  )
    void journal.refresh()
}
onMounted(async () => {
  document.documentElement.dataset.skin = uiPreferences.value.skin
  window.addEventListener('beforeunload', beforeUnload)
  window.addEventListener('focus', onFocus)
  window.addEventListener('keydown', onHistoryKeydown)
  window.addEventListener('pointermove', onLayoutPointerMove)
  window.addEventListener('pointerup', onLayoutPointerUp)
  window.addEventListener('keydown', onLayoutKeyDown)
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
    try {
      recent.value = await api.recent()
      unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
        event.preventDefault()
        if (
          busy.value ||
          picking.value ||
          settingsSaving.value ||
          settingsReading.value ||
          databaseBusy.value ||
          settingsLeaveBusy.value ||
          summaryExporting.value
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
  rangeRevision++
  settingsLoadRevision++
  window.removeEventListener('beforeunload', beforeUnload)
  window.removeEventListener('focus', onFocus)
  window.removeEventListener('keydown', onHistoryKeydown)
  window.removeEventListener('pointermove', onLayoutPointerMove)
  window.removeEventListener('pointerup', onLayoutPointerUp)
  window.removeEventListener('keydown', onLayoutKeyDown)
  clearInterval(clockTimer)
  unlistenClose?.()
  unlistenHistory?.()
  ai.close()
  ai.closeSummary()
  void excel.cancel()
  journal.dispose()
})
</script>

<template>
  <div class="app-shell">
    <div class="app-main">
      <AppToolbar
        :page="page"
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
        <Transition name="status-fade">
          <HolidayReminder
            v-if="holidayReminderVisible"
            :text="holidayReminderText"
            :disabled="holidayReminderDisabled"
            @open="openHolidaySettings"
            @dismiss="dismissHolidayReminder"
          />
        </Transition>
        <Transition name="page-switch">
          <div v-show="page === 'settings'" class="settings-viewport" :inert="page !== 'settings'">
            <Transition name="status-fade">
              <div v-if="error" class="settings-workspace-error error-banner" role="alert">
                {{ error }}<button class="text-action" @click="error = ''">关闭</button>
              </div>
            </Transition>
            <SettingsView
              ref="settingsView"
              :key="workspace?.path ?? 'no-workspace'"
              :backup-warning="backupWarning"
              :backup-retrying="backupRetrying"
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
              @retry-backup="journal.retryBackup"
              @database-busy="databaseActivity = $event"
              @database-transferred="onDatabaseTransferred"
              @sync-holidays="holidays.sync($event)"
              @back="navigate('home')"
              @workspace="chooseWorkspace"
            />
          </div>
        </Transition>
        <Transition name="page-switch">
          <main v-if="workspace" v-show="page === 'home'" class="home" :inert="page !== 'home'">
            <Transition name="status-fade">
              <div v-if="backupWarning" class="error-banner database-backup-warning" role="status">
                <span>{{ backupWarning }}</span>
                <md-text-button :disabled="backupRetrying || busy || databaseBusy" @click="journal.retryBackup">{{
                  backupRetrying ? '正在重试…' : '重试数据库备份'
                }}</md-text-button>
              </div>
            </Transition>
            <Transition name="status-fade">
              <div v-if="error" class="error-banner" role="alert">
                <span>{{ error }}</span>
                <md-text-button v-if="dirty" :disabled="saving" @click="journal.flush">重试保存</md-text-button>
                <md-icon-button v-else aria-label="关闭提示" @click="error = ''"><PhX :size="18" /></md-icon-button>
              </div>
            </Transition>
            <div
              ref="layoutEl"
              class="journal-layout"
              :class="[
                `layout-${uiPreferences.axis}`,
                { 'order-editor-first': uiPreferences.order[0] === 'editor', 'is-layout-dragging': !!layoutDragging },
              ]"
              :style="layoutStyle"
              @pointerdown="onLayoutPointerDown"
            >
              <CalendarPane
                ref="calendar"
                :style="{ order: paneOrder('calendar') }"
                :today="today"
                :selected="selectedDate"
                :entries="entries"
                :previews="previews"
                :disabled="busy || aiVisible || summaryVisible || requestingAi || excelBusy"
                @select="selectDate"
                @previews="loadPreviews"
                @range="loadRange"
              />
              <div
                v-if="showSplitter"
                class="journal-splitter"
                :class="{ 'is-dragging': resizing }"
                :style="{ order: 2 }"
                role="separator"
                aria-orientation="vertical"
                aria-label="调整日历和编辑区宽度"
                tabindex="0"
                title="拖动调整宽度，双击恢复默认"
                @pointerdown="startSplitterResize"
                @keydown="onSplitterKeydown"
                @dblclick="resetSplitterWidth"
              ></div>
              <div class="editor-stage" :style="{ order: paneOrder('editor') }">
                <Transition name="editor-switch">
                  <RangeOverview
                    v-if="rangeDates.length"
                    key="range"
                    v-model:preview="editorPreview"
                    :dates="rangeDates"
                    :documents="rangeDocuments"
                    :loading="rangeLoading"
                    :error="rangeError"
                    :summarizing="summaryVisible"
                    @close="closeRangeView"
                    @summarize="requestSummary(rangeDates)"
                  />
                  <JournalEditor
                    v-else
                    key="editor"
                    ref="editor"
                    v-model:preview="editorPreview"
                    :date="selectedDate"
                    :document="activeDocument"
                    :is-new="isNewDraft"
                    :content="draft"
                    :dirty="dirty"
                    :saving="saving"
                    :disabled="busy || aiVisible || summaryVisible || requestingAi || excelBusy"
                    :error="error"
                    :saved-at="savedAt"
                    :storage-mode="storageMode"
                    :can-restore="canRestore"
                    :can-undo="canUndo"
                    :can-redo="canRedo"
                    @edit="journal.edit"
                    @undo="journal.undo"
                    @redo="journal.redo"
                    @save="journal.flush"
                    @reload="requestReload"
                    @format="requestFormat"
                    @ai="requestAi"
                    @restore="requestRestore"
                    @error="journal.reportError"
                  />
                </Transition>
              </div>
              <div v-if="layoutDragging" class="layout-dropzones" aria-hidden="true">
                <div
                  v-for="zone in layoutZones"
                  :key="zone.id"
                  class="layout-dropzone"
                  :class="[zone.id, { active: layoutDropZone === zone.id }]"
                >
                  <span>{{ zone.label }}</span>
                </div>
                <p class="layout-drag-hint">拖到目标位置后松开，Esc 取消</p>
              </div>
            </div>
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
            v-if="!workspace && page === 'home'"
            :picking="picking"
            :busy="busy"
            :recent="recent"
            :recent-name="recentName"
            :error="error"
            @choose="chooseWorkspace"
            @open="journal.open"
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
      <AiSummaryDialog
        :visible="summaryVisible"
        :generating="summaryGenerating || requestingAi"
        :elapsed-seconds="summaryElapsedSeconds"
        :dates="summaryDates"
        :result="summaryResult"
        :error="summaryError"
        :provider="defaultProvider?.name ?? ''"
        :mode="summaryMode"
        :output-directory="summaryDirectory"
        :output-loading="summaryOutputLoading"
        :output-exporting="summaryExporting"
        :directory-opening="summaryDirectoryOpening"
        :output-error="summaryExportError || summaryDirectoryError || summaryOutputError"
        :exported-file="summaryExportResult?.filename ?? ''"
        :excel-enabled="excelSettings.enabled"
        :excel-preparing="excelPreparing || requestingExcel"
        :excel-exporting="excelExporting"
        :excel-preview="excelPreview"
        :excel-blocked-reason="excelBlockedReason"
        :excel-result="excelResult"
        :excel-error="excelExportError || excelDirectoryError || excelSettingsError"
        @excel="requestExcelExport"
        @excel-unavailable="reportExcelUnavailable"
        @cancel-excel="cancelExcelExport"
        @confirm-excel="confirmExcelPreview"
        @discard-excel="discardExcelPreview"
        @regenerate-excel="requestExcelExport"
        @open-excel-directory="openExcelOutputDirectory"
        @export="exportSummary"
        @open-directory="openSummaryDirectory"
        @close="ai.closeSummary"
        @ai="startAiSummary"
        @direct="ai.directSummary"
      />
      <AiPolishDialog
        ref="polishDialog"
        :visible="aiVisible"
        :generating="generating"
        :elapsed-seconds="elapsedSeconds"
        :original="aiOriginal"
        :result="aiResult"
        :error="aiResultError"
        :provider="defaultProvider?.name ?? ''"
        :provider-kind="defaultProvider?.kind ?? ''"
        @close="ai.close"
        @start="ai.generate"
        @regenerate="ai.generate"
        @edit="onAiResultEdit"
        @apply="applyAi(aiResult)"
        @restore="applyAi(aiOriginal, true)"
      />
      <SkinDialog ref="skinDialog" :active="uiPreferences.skin" @select="applySkin" @close="closeSkinDialog" />
    </div>
    <AppRail
      v-if="page === 'home' && workspace"
      :disabled="busy || excelBusy || picking"
      @refresh="refreshHome"
      @settings="navigate('settings')"
      @skin="openSkinDialog"
      @summary="openSummaryFromRail"
      @excel="openExcelFromRail"
      @history="openHistoryFromRail"
    />
  </div>
</template>
