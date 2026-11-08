<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { open as pickDirectory } from '@tauri-apps/plugin-dialog'
import { PhCircleNotch } from '@phosphor-icons/vue'
import AppToolbar from './components/AppToolbar.vue'
import AppRail from './components/AppRail.vue'
import SkinDialog from './components/SkinDialog.vue'
import GlobalSearchDialog from './components/GlobalSearchDialog.vue'
import ShortcutsDialog from './components/ShortcutsDialog.vue'
import SettingsView from './components/SettingsView.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import ModuleSettingsDialog from './components/ModuleSettingsDialog.vue'
import WorkspaceGate from './components/WorkspaceGate.vue'
import ConfirmDialog from './components/ConfirmDialog.vue'
import SettingsLeaveDialog from './components/SettingsLeaveDialog.vue'
import FileChangesDialog from './components/FileChangesDialog.vue'
import WorkspaceSwitchDialog from './components/WorkspaceSwitchDialog.vue'
import MessageHost from './components/MessageHost.vue'
import ButtonTooltip from './components/ButtonTooltip.vue'
import { api, type Document, type FileFormat } from './lib/workspace'
import { cheatsheetShortcut, historyShortcut, saveShortcut, searchShortcut } from './lib/shortcuts'
import { installContextMenuGuard } from './lib/contextMenus'
import TextContextMenu from './components/TextContextMenu.vue'
import { useJournal } from './lib/useJournal'
import { useWorkspaceDocument } from './lib/useWorkspaceDocument'
import { aiSummarySource, plainTextSummary } from './lib/summary'
import { useAi } from './lib/ai'
import ModuleBoard from './components/ModuleBoard.vue'
import LayoutDialog from './components/LayoutDialog.vue'
import type { DropSide, LayoutMetrics, ModuleId, ModuleType } from './modules/types'
import { moduleType } from './modules/types'
import { MODULE_CONTEXT, type CalendarHandle, type EditorHandle, type ModuleActions } from './modules/context'
import { useModuleLayout } from './modules/useModuleLayout'
import type { SkinId } from './lib/skins'
import { useAppearance } from './lib/useAppearance'
import { validExcelDates, useExcel } from './lib/useExcel'
import { useSummaryOutput } from './lib/useSummaryOutput'
import { workspaceFilesApi, type FileRequest, type FileChangeResult } from './lib/workspaceFiles'
import {
  settingsApi,
  cloneEditorSettings,
  DEFAULT_EDITOR_SETTINGS,
  type EditorSettings,
  type WorkspaceSettings,
} from './lib/settingsDraft'
import type { GlobalSettingsSection } from './lib/settingsSections'
import {
  cloneFileSettings,
  fileSettingsApi,
  FileChangesCancelled,
  type FileSettings,
  type FileChangesDecision,
} from './lib/fileSettings'
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
import { useGit } from './lib/useGit'
import { useUpdater } from './lib/useUpdater'
import UpdateDialog from './components/UpdateDialog.vue'

const nativeApp = isTauri()
const updater = useUpdater({ beforeInstall: prepareForUpdate })
const workspaceDocument = useWorkspaceDocument((): string | undefined => journal.workspace.value?.path)
const journal: ReturnType<typeof useJournal> = useJournal({ beforeFlush: workspaceDocument.flush })
let fileOpenRevision = 0
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
const git = useGit()
const { state: holidayState, loading: holidayLoading, syncing: holidaySyncing, error: holidayError } = holidays
const holidayReady = ref(false)
const { settings: summaryOutputSettings } = summaryOutput
const summaryWorkspacePath = ref('')
const { settings: excelSettings, preparing: excelPreparing, exporting: excelExporting } = excel
const {
  scale: appearanceScale,
  rail: appearanceRail,
  railHoldDelayMs,
  railHoldDurationMs,
  loading: appearanceLoading,
  error: appearanceError,
} = appearance
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
  moduleSettingsModule.value = null
  workspaceDocument.clear()
  fileOpenRevision++
  fileSettings.value = cloneFileSettings()
  editorSettings.value = cloneEditorSettings(DEFAULT_EDITOR_SETTINGS)
  journal.setEditorSettings(editorSettings.value)
  void loadWorkspaceSettings(true)
})
const { settings: aiSettings, error: aiSettingsError, defaultProvider, summaryVisible, summaryResult, summaryMode } = ai
watch(summaryResult, () => summaryOutput.clearExport())
const page = ref<'home' | 'settings'>('home')
const recent = ref<string | null>(null)
const startupPending = ref(nativeApp)
const picking = ref(false)
const workspaceSwitchDialog = ref<InstanceType<typeof WorkspaceSwitchDialog>>()
const pendingWorkspaceSwitch = ref<{ path: string; sourcePath: string; sourceName: string } | null>(null)
const workspaceSwitchError = ref('')
const requestingAi = ref(false)
const excelBusy = computed(() => excelPreparing.value || excelExporting.value)
const summarySelectionDates = ref<string[]>([])
const refreshRevision = ref(0)
const settingsView = ref<InstanceType<typeof SettingsView>>()
const moduleSettingsDialog = ref<InstanceType<typeof ModuleSettingsDialog>>()
/** 当前从模块标题栏打开设置的模块；为空表示没有模块设置弹窗。 */
const moduleSettingsModule = ref<ModuleType | null>(null)
const moduleSettingsSource = ref<ModuleId | null>(null)
const moduleSettingsVisible = computed(() => moduleSettingsModule.value !== null)
const moduleSettingsBusy = computed(() => settingsLoading.value || settingsSaving.value || databaseBusy.value)
const appRail = ref<InstanceType<typeof AppRail>>()
const settingsCloseTarget = ref<'settings' | 'close'>('settings')
function settingsButtonRect() {
  return appRail.value?.getSettingsRect() ?? null
}
const fileSettings = ref<FileSettings>(cloneFileSettings())
const settingsFilesError = ref('')
const fileChangesDialog = ref<InstanceType<typeof FileChangesDialog>>()
function acceptFileSettings(value: FileSettings) {
  fileSettings.value = cloneFileSettings(value)
  journal.setFileSettings(fileSettings.value)
  excel.acceptFileSettings(fileSettings.value.excel)
  summaryOutput.acceptFileSettings(fileSettings.value.summary)
}
const databaseSettings = ref(cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS))
const databaseLoading = ref(false)
const settingsDatabaseError = ref('')
const databaseActivity = ref(false)
const storageRefreshing = ref(false)
const databaseBusy = computed(() => databaseActivity.value || storageRefreshing.value || backupRetrying.value)
const storageMode = computed(() => (!databaseSettings.value.enabled ? 'files' : databaseSettings.value.mode))
const editorSettings = ref(cloneEditorSettings(DEFAULT_EDITOR_SETTINGS))
const editorLoading = ref(false)

// 新建日报沿用「数据存储」里选定的默认格式；已有日报不受影响。
watch(
  () => databaseSettings.value.defaultFormat,
  (format) => journal.setDefaultFormat(format),
  { immediate: true },
)

const workspaceSettings = computed<WorkspaceSettings>(() => ({
  appearance: {
    scale: appearanceScale.value,
    rail: appearanceRail.value,
    railHoldDelayMs: railHoldDelayMs.value,
    railHoldDurationMs: railHoldDurationMs.value,
  },
  ai: aiSettings.value,
  excel: excelSettings.value,
  summary: summaryOutputSettings.value,
  database: databaseSettings.value,
  git: git.settings.value,
  editor: editorSettings.value,
  files: fileSettings.value,
}))
const settingsReading = ref(false)
const settingsSaving = ref(false)
const settingsCommitting = computed(() => settingsSaving.value || (settingsView.value?.savingProxy ?? false))
const settingsReady = ref(false)
const settingsError = ref('')
const settingsLoading = computed(
  () =>
    settingsReading.value ||
    appearanceLoading.value ||
    excel.loading.value ||
    summaryOutput.loading.value ||
    databaseLoading.value ||
    git.loading.value ||
    editorLoading.value,
)
const settingsDialogBusy = computed(
  () =>
    busy.value ||
    picking.value ||
    settingsLoading.value ||
    settingsCommitting.value ||
    settingsLeaveBusy.value ||
    databaseBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value,
)
let settingsLoadRevision = 0
type SettingsExit = 'home' | 'workspace' | 'close'
const settingsLeaveDialog = ref<InstanceType<typeof SettingsLeaveDialog>>()
const skinDialog = ref<InstanceType<typeof SkinDialog>>()
const searchDialog = ref<InstanceType<typeof GlobalSearchDialog>>()
const shortcutsOpened = ref(false)
const pendingSettingsExit = ref<SettingsExit | null>(null)
const settingsLeaveBusy = ref(false)
const settingsLeaveError = ref('')
const confirmDialog = ref<InstanceType<typeof ConfirmDialog>>()
const modalKind = ref<'reload' | 'format' | 'restore'>('format')
const targetFormat = ref<FileFormat>('md')
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
/** 模块标题栏齿轮：按模块类型打开对应的模块设置弹窗。 */
function openModuleFromBoard(id: ModuleId) {
  void openModuleSettings(moduleType(id), id)
}
function moduleSettingsButtonRect() {
  return moduleSettingsSource.value ? (moduleBoard.value?.getSettingsRect(moduleSettingsSource.value) ?? null) : null
}
async function activateModule(id: ModuleId) {
  if (page.value !== 'home') {
    await navigate('home')
    if ((page.value as string) !== 'home') return
  }
  const existing = layoutItems.value.find((item) => moduleType(item.id) === moduleType(id))
  if (!existing) moduleLayout.add(id)
  await nextTick()
  moduleBoard.value?.reveal(existing?.id ?? id)
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
function resizeModules(path: number[], ratio: number, metrics: LayoutMetrics) {
  moduleLayout.resize(path, ratio, metrics)
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
function openSearchDialog() {
  if (
    !workspace.value ||
    page.value !== 'home' ||
    busy.value ||
    excelBusy.value ||
    picking.value ||
    moduleSettingsVisible.value
  )
    return
  searchDialog.value?.open()
}
function closeSearchDialog() {
  void searchDialog.value?.close()
}
async function onSearchSelectJournal(date: string) {
  await searchDialog.value?.close()
  const editorModule = layoutItems.value.find((item) => moduleType(item.id) === 'editor')
  if (!editorModule) moduleLayout.add('editor')
  await nextTick()
  moduleBoard.value?.reveal(editorModule?.id ?? 'editor', { focus: false })
  await selectReport(date)
}
async function onSearchSelectFile(path: string) {
  await searchDialog.value?.close()
  await openWorkspaceFile(path)
}
/** 目录或历史记录里选择单篇表示聚焦这一天：先解除框选，避免总结 / Excel 仍按旧范围统计。 */
async function selectReport(date: string) {
  if (journal.appendingGit.value) return
  if (date === selectedDate.value && !workspaceDocument.path.value && !rangeDates.value.length) return
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
  if (!validExcelDates(summarySelectionDates.value)) return '请选择有效的日期或日期范围。'
  if (!defaultProvider.value) return '请先在设置中配置并选择默认 AI。'
  if (!excelSettings.value.templatePath) return '请先完成 Excel 输出设置。'
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
let removeContextMenuGuard: (() => void) | undefined
let clockTimer: ReturnType<typeof setInterval> | undefined
let updateTimer: ReturnType<typeof setTimeout> | undefined
let appDisposed = false

async function prepareForUpdate(): Promise<boolean> {
  if (
    journal.appendingGit.value ||
    busy.value ||
    picking.value ||
    settingsLoading.value ||
    settingsCommitting.value ||
    settingsLeaveBusy.value ||
    databaseBusy.value ||
    requestingAi.value ||
    ai.summaryGenerating.value ||
    summaryOutput.exporting.value ||
    excelBusy.value ||
    moduleSettingsVisible.value ||
    pendingWorkspaceSwitch.value
  )
    return false
  if (!(await journal.flush())) throw new Error(error.value || '日报保存失败，请检查后重试。')
  if (page.value === 'settings' && settingsView.value && !settingsView.value.unchanged) {
    if (!(await settingsView.value.save())) throw new Error(settingsError.value || '设置保存失败，请检查设置后重试。')
  }
  return true
}

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

async function navigate(target: 'home' | 'settings', closeTarget: 'settings' | 'close' = 'settings') {
  // 模块设置弹窗是模态的；关闭它之后再切换页面。
  if (moduleSettingsModule.value) return
  if (
    busy.value ||
    picking.value ||
    settingsCommitting.value ||
    settingsReading.value ||
    databaseBusy.value ||
    settingsLeaveBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value ||
    page.value === target
  )
    return
  settingsCloseTarget.value = target === 'home' ? closeTarget : 'settings'
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
  if (moduleSettingsModule.value) return
  if (page.value !== 'settings') {
    await navigate('settings')
    return
  }
  if (
    !settingsView.value ||
    busy.value ||
    picking.value ||
    settingsLoading.value ||
    settingsCommitting.value ||
    settingsLeaveBusy.value ||
    databaseBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value
  )
    return
  settingsCloseTarget.value = 'settings'
  settingsLeaveBusy.value = true
  try {
    if (!settingsView.value.unchanged && !(await settingsView.value.save())) return
    if (await journal.flush()) page.value = 'home'
  } finally {
    settingsLeaveBusy.value = false
  }
}
/** 打开模块自己的设置弹窗；模块标题栏齿轮与模块内「设置」入口共用。 */
async function openModuleSettings(module: ModuleType, source: ModuleId = module) {
  if (busy.value || picking.value || settingsCommitting.value || databaseBusy.value || excelBusy.value) return
  if (page.value !== 'home') {
    await navigate('home')
    if ((page.value as string) !== 'home') return
  }
  settingsError.value = ''
  if (!settingsReady.value) await loadWorkspaceSettings(false)
  if (busy.value || picking.value) return
  moduleSettingsSource.value = source
  moduleSettingsModule.value = module
}
function closeModuleSettings() {
  moduleSettingsModule.value = null
}
/** 模块设置里点「前往模型配置」等入口时，先关闭弹窗再去全局设置对应分类。 */
async function openModuleGlobalSettings(section: GlobalSettingsSection) {
  moduleSettingsModule.value = null
  await nextTick()
  await navigate('settings')
  if (page.value === 'settings') await settingsView.value?.navigate(section)
}
async function saveModuleSettings(settings: WorkspaceSettings): Promise<boolean> {
  if (settingsCommitting.value || databaseBusy.value) return false
  const savedOk = await saveWorkspaceSettings(settings)
  if (savedOk) pushMessage({ tone: 'success', text: '设置已保存' })
  return savedOk
}
async function openHolidaySettings() {
  await openModuleSettings('calendar')
}
async function openAiSettings() {
  await navigate('settings')
  if ((page.value as string) === 'settings') await settingsView.value?.navigate('ai')
}
async function openExcelSettings() {
  await openModuleSettings('excel')
}
async function openGitSettings() {
  await openModuleSettings('git')
}
async function openEditorSettings() {
  await openModuleSettings('editor')
}
async function loadEditorSettings(): Promise<boolean> {
  const path = workspace.value?.path
  editorLoading.value = true
  try {
    const stored = nativeApp
      ? await invoke<EditorSettings>('load_editor_settings')
      : cloneEditorSettings(DEFAULT_EDITOR_SETTINGS)
    if (workspace.value?.path !== path) return false
    editorSettings.value = cloneEditorSettings(stored)
    journal.setEditorSettings(editorSettings.value)
    return true
  } catch {
    return false
  } finally {
    if (workspace.value?.path === path) editorLoading.value = false
  }
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
    git.load(),
    loadFileSettings(revision),
    loadEditorSettings(),
  ])
  if (revision !== settingsLoadRevision) return false
  settingsReading.value = false
  settingsReady.value = results.every(Boolean)
  if (results[6]) acceptFileSettings(fileSettings.value)
  if (!settingsReady.value)
    settingsError.value =
      [
        appearanceError.value,
        aiSettingsError.value,
        excel.error.value,
        summaryOutput.error.value,
        settingsDatabaseError.value,
        settingsFilesError.value,
        git.error.value,
      ]
        .filter(Boolean)
        .join('；') || '读取设置失败，请重新进入设置后重试。'
  return settingsReady.value
}
async function loadFileSettings(revision: number): Promise<boolean> {
  const path = workspace.value?.path
  settingsFilesError.value = ''
  try {
    const stored = nativeApp ? await fileSettingsApi.load() : cloneFileSettings()
    if (workspace.value?.path !== path || revision !== settingsLoadRevision) return false
    fileSettings.value = cloneFileSettings(stored)
    return true
  } catch (cause) {
    if (workspace.value?.path === path && revision === settingsLoadRevision)
      settingsFilesError.value = `读取目录设置失败：${String(cause)}`
    return false
  }
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
  if (settingsCommitting.value || databaseBusy.value) return false
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
    const proposed = cloneFileSettings(value.files, value.summary.outputDirectory)
    const filesChanged = JSON.stringify(proposed) !== JSON.stringify(fileSettings.value)
    let fileChanges: FileChangesDecision | undefined
    if (filesChanged) {
      const plan = await fileSettingsApi.preview(path, proposed)
      if (workspace.value?.path !== path) return false
      let renameExisting = { journal: false, excel: false, summary: false }
      if (
        plan.directories.some((change) => change.fileCount > 0 || (change.targetFileCount ?? 0) > 0) ||
        plan.filenames.length
      ) {
        const decision = await fileChangesDialog.value?.confirm(plan)
        if (!decision) throw new FileChangesCancelled()
        renameExisting = decision
      }
      fileChanges = { token: plan.token, renameExisting }
    }
    const stored = await settingsApi.save(path, { ...value, files: proposed }, fileChanges)
    if (workspace.value?.path !== path) return false
    ai.acceptSaved(stored.ai)
    appearance.acceptSaved(stored.appearance)
    excel.acceptSaved(stored.excel)
    summaryOutput.acceptSaved(stored.summary)
    acceptFileSettings(cloneFileSettings(stored.files, stored.summary.outputDirectory))
    git.acceptSaved(stored.git)
    databaseSettings.value = cloneDatabaseSettings(stored.database)
    editorSettings.value = cloneEditorSettings(stored.editor)
    journal.setEditorSettings(editorSettings.value)
    if (filesChanged) workspaceDocument.clear()
    if (storageChanged || filesChanged) {
      clearRange()
      calendarHandle.value?.clearRange()
      await journal.reload()
    }
    return true
  } catch (cause) {
    if (cause instanceof FileChangesCancelled) throw cause
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
  if (settingsLeaveBusy.value || settingsCommitting.value) return
  pendingSettingsExit.value = null
  settingsCloseTarget.value = 'settings'
  settingsLeaveError.value = ''
  void settingsLeaveDialog.value?.close()
}
async function finishSettingsExit(save: boolean) {
  if (!pendingSettingsExit.value || settingsLeaveBusy.value || settingsCommitting.value) return
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
  if (excelExporting.value || journal.appendingGit.value) return
  rangeRevision++
  ai.closeSummary()
  await excel.cancel()
  if (await journal.flush()) await getCurrentWindow().destroy()
}
async function chooseWorkspace() {
  if (
    picking.value ||
    busy.value ||
    settingsCommitting.value ||
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
  if (excelBusy.value || busy.value) return
  const intent = ++fileOpenRevision
  if (!(await workspaceDocument.flush()) || intent !== fileOpenRevision) return
  workspaceDocument.clear()
  await journal.select(date)
}
async function openWorkspaceFile(path: string) {
  if (busy.value || excelBusy.value || page.value !== 'home') return
  const currentWorkspace = workspace.value?.path
  const intent = ++fileOpenRevision
  if (!(await journal.flush()) || intent !== fileOpenRevision || workspace.value?.path !== currentWorkspace) return
  clearRange()
  calendarHandle.value?.clearRange()
  const editorModule = layoutItems.value.find((item) => moduleType(item.id) === 'editor')
  if (!editorModule) moduleLayout.add('editor')
  await nextTick()
  if (intent !== fileOpenRevision) return
  moduleBoard.value?.reveal(editorModule?.id ?? 'editor', { focus: false })
  const value = await workspaceDocument.open(path)
  if (intent !== fileOpenRevision) return
  if (value?.journalDate) {
    workspaceDocument.clear()
    await selectReport(value.journalDate)
  }
}
async function showJournal() {
  if (busy.value || !(await journal.flush())) return
  fileOpenRevision++
  workspaceDocument.clear()
  await journal.reload(true)
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
    if (workspaceDocument.path.value) await workspaceDocument.discardAndReload()
    else await journal.reload()
    void confirmDialog.value?.close()
  }
}
function cancelConfirm() {
  if (!busy.value) void confirmDialog.value?.close()
}
function clearRange() {
  excel.clearSelection()
  rangeRevision++
  rangeDates.value = []
  rangeDocuments.value = []
  rangeLoading.value = false
  rangeError.value = ''
}
async function loadRange(dates: string[], duringGitAppend = false) {
  if (excelBusy.value || (journal.appendingGit.value && !duringGitAppend)) return
  if (!dates.length) {
    clearRange()
    return
  }
  const revision = ++rangeRevision
  fileOpenRevision++
  if (!(await journal.flush()) || revision !== rangeRevision) return
  workspaceDocument.clear()
  excel.clearSelection()
  rangeDates.value = [...dates].sort()
  rangeDocuments.value = []
  rangeError.value = ''
  rangeLoading.value = true
  try {
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
  else if (workspaceDocument.path.value) await workspaceDocument.reload()
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
  if (workspaceDocument.path.value) {
    if (workspaceDocument.document.value?.kind === 'text') {
      if (action === 'undo') workspaceDocument.undo()
      else workspaceDocument.redo()
    }
  } else if (action === 'undo') journal.undo()
  else journal.redo()
}
function onHistoryKeydown(event: KeyboardEvent) {
  // WebKit can deliver shortcuts directly even when the native Edit menu is
  // installed. Handle those DOM events as well as the native menu events.
  if (event.isComposing || event.keyCode === 229 || event.defaultPrevented) return
  if (searchShortcut(event, isMac)) {
    if (searchDialog.value?.isOpen()) {
      event.preventDefault()
      closeSearchDialog()
      return
    }
    if (
      workspace.value &&
      page.value === 'home' &&
      !busy.value &&
      !excelBusy.value &&
      !picking.value &&
      !moduleSettingsVisible.value &&
      !document.querySelector('dialog[open]:not(.is-inline)')
    ) {
      event.preventDefault()
      openSearchDialog()
    }
    return
  }
  if (saveShortcut(event, isMac)) {
    if (document.querySelector('dialog[open]:not(.is-inline)')) return
    event.preventDefault()
    if (page.value === 'settings') {
      void settingsView.value?.save()
      return
    }
    if (workspaceDocument.path.value) {
      void workspaceDocument.flush()
    } else {
      void journal.flush()
    }
    pushMessage({ tone: 'success', text: '草稿已立即保存' })
    return
  }
  if (cheatsheetShortcut(event, isMac)) {
    if (!shortcutsOpened.value && document.querySelector('dialog[open]:not(#shortcuts-dialog):not(.is-inline)')) {
      return
    }
    event.preventDefault()
    shortcutsOpened.value = !shortcutsOpened.value
    return
  }
  if (event.key === '?' && !event.metaKey && !event.ctrlKey && !event.altKey) {
    const target = event.target
    if (
      !(target instanceof HTMLElement && target.closest('input, textarea, [contenteditable="true"]')) &&
      page.value === 'home' &&
      !document.querySelector('dialog[open]:not(.is-inline)')
    ) {
      event.preventDefault()
      shortcutsOpened.value = true
      return
    }
  }
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
    [...ai.summaryScopeDates.value],
    summaryResult.value,
    format,
    summaryMode.value,
  )
}
async function prepareExcel(dates: string[], prompt?: string) {
  if (busy.value || excelBusy.value || !workspace.value) return
  const workspacePath = workspace.value.path
  summaryWorkspacePath.value = workspacePath
  summarySelectionDates.value = [...dates].sort()
  excel.exportError.value = ''
  if (!excelSettings.value.enabled) {
    excel.exportError.value = '请先在 Excel 设置中启用输出。'
    return
  }
  const blockedReason = excelBlockedReason.value
  if (blockedReason) {
    excel.exportError.value = blockedReason
    return
  }
  await excel.prepareSelection(workspacePath, summarySelectionDates.value, prompt)
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
let summaryGenerationRevision = 0
function cancelAiSummary() {
  summaryGenerationRevision++
  requestingAi.value = false
  ai.cancelSummary()
}
async function startAiSummary(prompt?: string) {
  if (requestingAi.value || ai.summaryGenerating.value || excelBusy.value || !summaryVisible.value) return
  const revision = ++summaryGenerationRevision
  const workspacePath = workspace.value?.path
  const scope = ai.summaryScopeDates.value.join(',')
  requestingAi.value = true
  try {
    const loaded = await ai.load()
    if (
      revision !== summaryGenerationRevision ||
      workspacePath !== workspace.value?.path ||
      scope !== ai.summaryScopeDates.value.join(',') ||
      !summaryVisible.value
    )
      return
    if (!loaded) {
      ai.summaryError.value = `无法读取 AI 设置：${aiSettingsError.value}`
      return
    }
    if (!defaultProvider.value) {
      ai.summaryError.value = '请先在设置中配置并选择默认 AI。'
      return
    }
    requestingAi.value = false
    await ai.generateSummary(prompt)
  } finally {
    if (revision === summaryGenerationRevision) requestingAi.value = false
  }
}
async function appendGitReport(workspacePath: string, date: string, content: string): Promise<boolean> {
  if (
    !content.trim() ||
    busy.value ||
    saving.value ||
    settingsLoading.value ||
    settingsCommitting.value ||
    page.value !== 'home' ||
    workspace.value?.path !== workspacePath ||
    selectedDate.value !== date ||
    activeDocument.value?.date !== date ||
    rangeDates.value.length
  )
    return false
  const existing = journal.draft.value
  if (existing.includes(content.trim())) {
    pushMessage({ tone: 'info', text: '当前日报已包含这份 Git 工作记录。' })
    return false
  }
  const separator = !existing ? '' : existing.endsWith('\n\n') ? '' : existing.endsWith('\n') ? '\n' : '\n\n'
  journal.edit(`${existing}${separator}${content.trim()}`, true)
  const saved = await journal.flush()
  if (saved) pushMessage({ tone: 'success', text: 'Git 工作记录已追加到日报并保存' })
  return saved
}
const moduleDisabled = computed(() => page.value !== 'home' || busy.value || requestingAi.value)
async function changeWorkspaceFiles(path: string, request: FileRequest, token?: string): Promise<FileChangeResult> {
  if (
    busy.value ||
    settingsCommitting.value ||
    settingsReading.value ||
    databaseBusy.value ||
    excelBusy.value ||
    summaryOutput.exporting.value ||
    requestingAi.value ||
    ai.summaryGenerating.value
  )
    throw new Error('当前任务尚未完成，请稍后再操作文件。')
  if (workspace.value?.path !== path) throw new Error('工作空间已切换，请重新打开操作窗口。')
  busy.value = true
  let result: FileChangeResult
  try {
    if (!(await journal.flush())) throw new Error(error.value || '请先保存当前日报。')
    if (workspace.value?.path !== path) throw new Error('工作空间已切换。')
    const approved = token ?? (await workspaceFilesApi.preview(path, request)).token
    result = await workspaceFilesApi.apply(path, request, approved)
    if (workspace.value?.path !== path) throw new Error('文件已处理，但工作空间已切换，请刷新目录。')
    acceptFileSettings(result.files)
    if (request.action === 'rename') workspaceDocument.remap(request.path, result.path)
    if (request.action === 'delete') workspaceDocument.remap(request.path, null)
    excel.clearExport()
    summaryOutput.clearExport()
  } finally {
    busy.value = false
  }
  await journal.reload(true)
  if (workspaceDocument.path.value) {
    const value = await workspaceDocument.reload()
    if (value?.journalDate) {
      workspaceDocument.clear()
      await selectReport(value.journalDate)
    }
  }
  if (rangeDates.value.length) await loadRange([...rangeDates.value], true)
  if (layoutItems.value.some((item) => moduleType(item.id) === 'excel')) await excel.loadExports(path)
  refreshRevision.value++
  return result
}

const moduleActions: ModuleActions = {
  openWorkspaceFile,
  showJournal,
  changeWorkspaceFiles,
  selectDate,
  selectReport,
  loadRange,
  requestSummary,
  generateSummary: startAiSummary,
  cancelSummary: cancelAiSummary,
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
  openGitSettings: () => {
    void openGitSettings()
  },
  openEditorSettings: () => {
    void openEditorSettings()
  },
  openShortcuts: () => {
    shortcutsOpened.value = true
  },
  appendGitReport,
  syncHolidays: (year: number) => {
    void syncHolidays(year)
  },
}
provide(MODULE_CONTEXT, {
  journal,
  workspaceDocument,
  ai,
  holidays,
  excel,
  summaryOutput,
  fileSettings,
  editorSettings,
  git,
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
watch(workspaceDocument.error, (value) => {
  if (!value) return
  pushMessage({
    tone: 'error',
    text: value,
    duration: workspaceDocument.dirty.value ? 0 : undefined,
    actions: workspaceDocument.dirty.value
      ? [{ label: '重试保存文件', run: () => void workspaceDocument.flush() }]
      : undefined,
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
    if (value?.filename) pushMessage({ tone: 'success', text: `Excel报告已生成：${value.filename}` })
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
    journal.appendingGit.value ||
    updater.busy.value ||
    dirty.value ||
    saving.value ||
    workspaceDocument.dirty.value ||
    workspaceDocument.saving.value ||
    settingsCommitting.value ||
    databaseBusy.value ||
    summaryOutput.exporting.value ||
    excelBusy.value ||
    (moduleSettingsVisible.value && moduleSettingsDialog.value?.unchanged === false) ||
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
    !updater.busy.value &&
    !saving.value &&
    !error.value &&
    !confirmDialog.value?.isOpen() &&
    !picking.value &&
    !excelBusy.value &&
    !rangeDates.value.length &&
    !databaseBusy.value
  ) {
    if (workspaceDocument.path.value) {
      if (
        !workspaceDocument.dirty.value &&
        !workspaceDocument.saving.value &&
        workspaceDocument.document.value?.kind === 'text'
      )
        void workspaceDocument.reload()
    } else void journal.refresh()
  }
}
onMounted(async () => {
  removeContextMenuGuard = installContextMenuGuard()
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
    if (import.meta.env.PROD) updateTimer = setTimeout(() => void updater.checkForUpdates(false), 3000)
    void ai.load()
    void restoreRecentWorkspace()
    try {
      unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
        event.preventDefault()
        if (fileChangesDialog.value?.isOpen()) {
          await fileChangesDialog.value.close()
          return
        }
        if (pendingWorkspaceSwitch.value) {
          cancelWorkspaceSwitch()
          return
        }
        // 模块设置弹窗优先关闭；有未保存修改时先弹出弹窗内的放弃确认。
        if (moduleSettingsVisible.value) {
          moduleSettingsDialog.value?.requestClose()
          return
        }
        if (
          journal.appendingGit.value ||
          updater.busy.value ||
          busy.value ||
          picking.value ||
          settingsCommitting.value ||
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
  removeContextMenuGuard?.()
  clearInterval(clockTimer)
  clearTimeout(updateTimer)
  updater.dispose()
  unlistenClose?.()
  unlistenHistory?.()
  ai.closeSummary()
  void excel.cancel()
  journal.dispose()
  workspaceDocument.dispose()
  clearMessages()
})
</script>

<template>
  <div
    class="app-shell"
    :class="{
      'has-native-titlebar': nativeApp && isMac,
      'rail-left': appearanceRail === 'left',
      'settings-open': page === 'settings',
    }"
  >
    <div class="app-main">
      <AppToolbar
        :inert="page === 'settings' || moduleSettingsVisible"
        :page="page"
        :native-titlebar="nativeApp && isMac"
        :is-mac="isMac"
        :greeting="greeting"
        :date-label="now.format('M 月 D 日 dddd')"
        :has-workspace="!!workspace"
        :workspace-name="workspace?.name"
        :workspace-path="workspace?.path"
        :busy="busy"
        :picking="picking"
        :excel-busy="excelBusy"
        :settings-saving="settingsCommitting"
        :settings-reading="settingsReading"
        :settings-leave-busy="settingsLeaveBusy"
        :database-busy="databaseBusy"
        :refresh-revision="refreshRevision"
        @navigate="navigate"
        @refresh="refreshHome"
        @workspace="chooseWorkspace"
        @search="openSearchDialog"
        @shortcuts="shortcutsOpened = true"
      />
      <div class="app-content">
        <main v-if="startupPending" class="workspace-loading" role="status" aria-live="polite">
          <PhCircleNotch class="spin" :size="18" aria-hidden="true" /><span>正在打开工作空间…</span>
        </main>
        <Transition name="page-switch">
          <main v-if="workspace && !startupPending" class="home" :inert="page !== 'home' || moduleSettingsVisible">
            <ModuleBoard
              ref="moduleBoard"
              :layout="layoutTree"
              :workspace-path="workspace.path"
              @split="splitModule"
              @insert="insertModuleAtGutter"
              @edge="moveModuleToEdge"
              @resize="resizeModules"
              @remove="removeModule"
              @settings="openModuleFromBoard"
            />
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
      <footer v-if="workspace && !startupPending" class="app-statusbar" aria-label="工作空间状态">
        <span class="app-statusbar-path" :title="workspace.path">{{ workspace.path }}</span>
        <span>{{ entries.length }} 篇日报</span>
        <span class="app-statusbar-document" :title="workspaceDocument.path.value || selectedDate">{{
          workspaceDocument.path.value || activeDocument?.filename || selectedDate
        }}</span>
        <span v-if="!workspaceDocument.path.value && activeDocument">{{
          activeDocument.format === 'txt' ? '纯文本' : 'Markdown'
        }}</span>
      </footer>
      <SettingsDialog
        :visible="page === 'settings'"
        :busy="settingsDialogBusy"
        :button-rect="settingsButtonRect"
        :close-target="settingsCloseTarget"
        @close="navigate('home', $event)"
      >
        <div class="settings-viewport" :inert="page !== 'settings'">
          <SettingsView
            ref="settingsView"
            :key="workspace?.path ?? 'no-workspace'"
            :workspace="workspace"
            :picking="picking || busy || excelBusy || settingsCommitting"
            :bundle="workspaceSettings"
            :loading="settingsLoading"
            :saving="settingsSaving"
            :database-busy="databaseBusy"
            :before-database-transfer="journal.flush"
            :error="settingsError"
            :save="saveWorkspaceSettings"
            :updater="updater"
            @database-busy="databaseActivity = $event"
            @database-transferred="onDatabaseTransferred"
            @back="navigate('home')"
            @workspace="chooseWorkspace"
          />
        </div>
      </SettingsDialog>
      <ModuleSettingsDialog
        ref="moduleSettingsDialog"
        :visible="moduleSettingsVisible"
        :module="moduleSettingsModule"
        :button-rect="moduleSettingsButtonRect"
        :workspace="workspace"
        :bundle="workspaceSettings"
        :busy="moduleSettingsBusy"
        :error="settingsError"
        :save="saveModuleSettings"
        :holidays="holidayState"
        :holiday-loading="holidayLoading"
        :holiday-syncing="holidaySyncing"
        :holiday-error="holidayError"
        :current-year="now.year()"
        @close="closeModuleSettings"
        @open-global-settings="openModuleGlobalSettings"
        @sync-holidays="syncHolidays"
      />
      <FileChangesDialog ref="fileChangesDialog" />
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
        :filename="workspaceDocument.path.value ? workspaceDocument.document.value?.filename : activeDocument?.filename"
        :date="selectedDate"
        :storage-mode="workspaceDocument.path.value ? 'files' : storageMode"
        :error="workspaceDocument.path.value ? workspaceDocument.error.value : error"
        :busy="busy"
        @confirm="confirmModal"
        @cancel="cancelConfirm"
      />
      <SkinDialog ref="skinDialog" :active="activeSkin" @select="applySkin" @close="closeSkinDialog" />
      <GlobalSearchDialog
        ref="searchDialog"
        :workspace-path="workspace?.path"
        :entries="entries"
        :is-mac="isMac"
        @select-journal="onSearchSelectJournal"
        @select-file="onSearchSelectFile"
        @close="closeSearchDialog"
      />
      <ShortcutsDialog :visible="shortcutsOpened" @close="shortcutsOpened = false" />
      <LayoutDialog
        ref="layoutDialog"
        :saved-layouts="savedLayouts"
        @reset="resetLayout"
        @save="saveCurrentLayout"
        @apply="applySavedLayout"
        @preset="moduleLayout.applyPreset"
        @remove="removeSavedLayout"
        @close="closeLayoutDialog"
      />
    </div>
    <AppRail
      v-if="workspace"
      :key="workspace.path"
      ref="appRail"
      :disabled="busy || excelBusy || picking || moduleSettingsVisible"
      :active="layoutItems.map((item) => item.id)"
      :hold-delay-ms="railHoldDelayMs"
      :hold-duration-ms="railHoldDurationMs"
      :settings-active="page === 'settings'"
      :settings-busy="
        settingsLoading || settingsCommitting || settingsLeaveBusy || databaseBusy || moduleSettingsVisible
      "
      @module="activateModule"
      @remove="removeModule"
      @refresh="refreshHome"
      @layout="openLayoutFromRail"
      @settings="toggleSettings"
      @skin="openSkinDialog"
    />
    <UpdateDialog :updater="updater" />
    <MessageHost />
    <TextContextMenu :scope="`${documentKey}/${page}`" />
    <ButtonTooltip />
  </div>
</template>
