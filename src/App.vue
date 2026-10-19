<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import dayjs from 'dayjs'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open as pickDirectory } from '@tauri-apps/plugin-dialog'
import { PhCalendarDots, PhFolderOpen, PhArrowRight, PhArrowClockwise, PhX, PhFolder, PhFileText } from '@phosphor-icons/vue'
import CalendarPane from './components/CalendarPane.vue'
import JournalEditor from './components/JournalEditor.vue'
import SettingsView from './components/SettingsView.vue'
import AiPolishDialog from './components/AiPolishDialog.vue'
import { api, type FileFormat } from './lib/workspace'
import { useJournal } from './lib/useJournal'
import { useAi } from './lib/ai'

const journal = useJournal()
const { workspace, entries, selectedDate, document: activeDocument, draft, dirty, busy, saving, error, savedAt } = journal
const ai = useAi()
const { settings: aiSettings, error: aiSettingsError, saving: aiSaving, settingsSaved, defaultProvider, visible: aiVisible, generating, original: aiOriginal, result: aiResult, resultError: aiResultError } = ai
const page = ref<'home' | 'settings'>('home')
const recent = ref<string | null>(null)
const picking = ref(false)
const openingDirectory = ref(false)
const requestingAi = ref(false)
const settingsView = ref<InstanceType<typeof SettingsView>>()
const modal = ref<HTMLDialogElement>()
const modalKind = ref<'create' | 'reload' | 'format' | 'restore'>('create')
const modalDate = ref('')
const createFormat = ref<FileFormat>('md')
const targetFormat = ref<FileFormat>('md')
const editor = ref<InstanceType<typeof JournalEditor>>()
const now = ref(dayjs())
const nativeApp = isTauri()
const recentName = computed(() => recent.value?.split(/[\\/]/).filter(Boolean).at(-1))
const greeting = computed(() => now.value.hour() < 12 ? '上午好' : now.value.hour() < 18 ? '下午好' : '晚上好')
const documentKey = computed(() => `${workspace.value?.path ?? ''}/${selectedDate.value}`)
const backups = ref<Record<string, string>>({})
const canRestore = computed(() => backups.value[documentKey.value] !== undefined)
let aiDocumentKey = ''
let unlistenClose: (() => void) | undefined
let clockTimer: ReturnType<typeof setInterval> | undefined

async function navigate(target: 'home' | 'settings') {
  if (busy.value || aiVisible.value || !(await journal.flush())) return
  if (page.value === 'settings' && !(await settingsView.value?.flush())) return
  page.value = target
}
async function openWorkspaceDirectory() {
  if (!workspace.value || openingDirectory.value) return
  openingDirectory.value = true
  try { await api.openDirectory() }
  catch (cause) { journal.reportError(cause) }
  finally { openingDirectory.value = false }
}
async function chooseWorkspace() {
  if (picking.value || busy.value) return
  if (!nativeApp) { journal.reportError('请在 Tauri 桌面应用中选择本地文件夹。使用 npm run app:dev 启动。'); return }
  if (!(await journal.flush())) return
  picking.value = true
  try {
    const path = await pickDirectory({ directory: true, multiple: false, title: '选择 WorkReport 工作空间', defaultPath: workspace.value?.path ?? recent.value ?? undefined })
    if (typeof path === 'string' && await journal.open(path)) recent.value = workspace.value?.path ?? path
  } catch (cause) { journal.reportError(cause) }
  finally { picking.value = false }
}
function showConfirmation() {
  const dialog = modal.value
  if (!dialog || dialog.open) return
  dialog.showModal()
  dialog.focus({ preventScroll: true })
}
async function requestCreate(date: string) {
  if (!workspace.value || busy.value || !(await journal.select(date))) return
  if (activeDocument.value) { editor.value?.focus(); return }
  modalKind.value = 'create'; modalDate.value = date; createFormat.value = 'md'
  await nextTick(); showConfirmation()
}
function requestFormat(format: FileFormat) { targetFormat.value = format; modalKind.value = 'format'; showConfirmation() }
function requestReload() { modalKind.value = 'reload'; showConfirmation() }
function requestRestore() { modalKind.value = 'restore'; showConfirmation() }
async function confirmModal() {
  if (modalKind.value === 'create') {
    if (await journal.create(modalDate.value, createFormat.value)) { modal.value?.close(); editor.value?.focus() }
  } else if (modalKind.value === 'format') {
    if (await journal.switchFormat(targetFormat.value)) { modal.value?.close(); editor.value?.focus() }
  } else if (modalKind.value === 'restore') {
    const original = backups.value[documentKey.value]
    if (original !== undefined) { journal.edit(original); if (!(await journal.flush())) return; delete backups.value[documentKey.value] }
    modal.value?.close()
  } else { await journal.reload(); modal.value?.close() }
}
async function requestAi() {
  if (requestingAi.value || !activeDocument.value || busy.value) return
  requestingAi.value = true
  try {
    if (!(await ai.load())) { journal.reportError(`无法读取 AI 设置：${aiSettingsError.value}`); return }
    if (!defaultProvider.value) { await navigate('settings'); return }
    if (!draft.value.trim()) { journal.reportError('请先写下日报内容，再使用 AI 润色。'); return }
    if (!(await journal.flush())) return
    aiDocumentKey = documentKey.value
    ai.open(draft.value, activeDocument.value.format)
  } finally { requestingAi.value = false }
}
async function applyAi(content: string, restore = false) {
  if (documentKey.value !== aiDocumentKey) { ai.close(); journal.reportError('日报已切换，未应用润色结果。'); return }
  if (!restore) backups.value[documentKey.value] ??= aiOriginal.value
  journal.edit(content)
  ai.close()
  await journal.flush()
  editor.value?.focus()
}
function beforeUnload(event: BeforeUnloadEvent) { if (dirty.value || saving.value || aiSaving.value || (page.value === 'settings' && !settingsView.value?.unchanged)) { event.preventDefault(); event.returnValue = '' } }
function onFocus() {
  now.value = dayjs()
  if (!dirty.value && !saving.value && !error.value && !modal.value?.open && !picking.value && !aiVisible.value) void journal.refresh()
}
onMounted(async () => {
  window.addEventListener('beforeunload', beforeUnload); window.addEventListener('focus', onFocus)
  clockTimer = setInterval(() => { now.value = dayjs() }, 60_000)
  if (nativeApp) {
    void ai.load()
    try {
      recent.value = await api.recent()
      unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => { event.preventDefault(); if (busy.value) return; ai.close(); if (page.value === 'settings' && !(await settingsView.value?.flush())) return; if (await journal.flush()) await getCurrentWindow().destroy() })
    } catch (cause) { journal.reportError(cause) }
  }
})
onBeforeUnmount(() => { window.removeEventListener('beforeunload', beforeUnload); window.removeEventListener('focus', onFocus); clearInterval(clockTimer); unlistenClose?.(); ai.close(); journal.dispose() })
</script>

<template>
  <div class="app-shell">
    <header class="app-header"><a class="brand" href="#" aria-label="WorkReport 首页" @click.prevent="navigate('home')"><span class="brand-mark"><PhCalendarDots :size="26" weight="bold" /></span><span>WorkReport<span class="brand-subtitle">工作日报</span></span></a><button class="text-action header-settings" :disabled="busy" @click="navigate(page === 'settings' ? 'home' : 'settings')">{{ page === 'settings' ? '首页' : '设置' }}</button></header>
    <div class="app-content">
    <div v-show="page === 'settings'" class="page-scroll">
      <SettingsView ref="settingsView" :workspace="workspace" :picking="picking || busy" :settings="aiSettings" :saving="aiSaving" :error="aiSettingsError" :saved="settingsSaved" :save="ai.save" @back="navigate('home')" @workspace="chooseWorkspace" />
      <div v-if="error" class="settings-workspace-error error-banner" role="alert">{{ error }}<button class="text-action" @click="error = ''">关闭</button></div>
    </div>
    <main v-if="workspace" v-show="page === 'home'" class="home">
      <div class="home-heading"><div><p>{{ greeting }}，今天是 {{ now.format('M 月 D 日 dddd') }}</p><h1>把今天的进展，记下来。</h1></div><div class="local-workspace-tools"><span class="local-badge"><span></span>本地工作空间</span><button class="text-action workspace-refresh" :disabled="busy || picking" @click="journal.refresh"><PhArrowClockwise :size="13" />刷新</button></div></div>
      <div v-if="error" class="error-banner" role="alert"><span>{{ error }}</span><md-text-button v-if="dirty" :disabled="saving" @click="journal.flush">重试保存</md-text-button><md-icon-button v-else aria-label="关闭提示" @click="error = ''"><PhX :size="18" /></md-icon-button></div>
      <div class="journal-layout"><CalendarPane :selected="selectedDate" :entries="entries" :disabled="busy || aiVisible || requestingAi" @select="journal.select" @create="requestCreate" /><JournalEditor ref="editor" :date="selectedDate" :document="activeDocument" :content="draft" :dirty="dirty" :saving="saving" :disabled="busy || aiVisible || requestingAi" :error="error" :saved-at="savedAt" :can-restore="canRestore" @edit="journal.edit" @create="requestCreate(selectedDate)" @save="journal.flush" @reload="requestReload" @format="requestFormat" @ai="requestAi" @restore="requestRestore" @error="journal.reportError" /></div>
      <footer class="home-footer"><span :title="workspace.path"><button class="workspace-folder-button" aria-label="打开工作空间目录" title="打开工作空间目录" :disabled="openingDirectory" @click="openWorkspaceDirectory"><PhFolder :size="15" /></button><span class="workspace-path-text">{{ workspace.path }}</span></span><span>每一天，一份工作记录</span></footer>
    </main>
    <main v-else-if="page === 'home'" class="workspace-gate"><div class="welcome-art" aria-hidden="true"><div class="art-folder"><PhFolderOpen :size="68" weight="light" /></div><span class="art-file"><PhFileText :size="30" weight="light" /></span></div><span class="welcome-label">你的工作，从这里开始</span><h1>为日报，选一个家。</h1><p>选择一个文件夹作为工作空间。<br />每天的记录，保存为 Markdown 或 TXT 文件。</p><md-filled-button :disabled="picking || busy" @click="chooseWorkspace"><PhFolderOpen slot="icon" :size="19" />{{ picking || busy ? '正在打开…' : '选择工作空间' }}</md-filled-button><button v-if="recent" class="recent-workspace" :disabled="picking || busy" :title="recent" @click="journal.open(recent)"><PhFolder :size="18" /><span><small>继续使用上次的工作空间</small><strong>{{ recentName }}</strong></span><PhArrowRight :size="18" /></button><p v-if="error" class="gate-error" role="alert">{{ error }}</p><div class="welcome-footnote">文件保存在本地，随时可以用其他编辑器打开。</div></main>
    </div>
    <dialog ref="modal" tabindex="-1" role="dialog" aria-modal="true" class="confirmation-dialog" aria-labelledby="dialog-title" aria-describedby="dialog-description" @cancel="busy && $event.preventDefault()"><div class="dialog-icon"><PhFileText :size="28" weight="light" /></div><h2 id="dialog-title">{{ modalKind === 'create' ? '新增这一天的日报？' : modalKind === 'format' ? '切换日报文件格式？' : modalKind === 'restore' ? '恢复润色前的原文？' : '重新载入磁盘内容？' }}</h2>
      <template v-if="modalKind === 'create'"><p id="dialog-description">{{ dayjs(modalDate).format('YYYY 年 M 月 D 日 dddd') }}<br />将在工作空间中创建 <strong>{{ modalDate }}.{{ createFormat }}</strong>。</p><fieldset class="create-format-choices"><legend>文件格式</legend><label><input v-model="createFormat" type="radio" value="md" name="create-format" />Markdown <span>.md</span></label><label><input v-model="createFormat" type="radio" value="txt" name="create-format" />纯文本 <span>.txt</span></label></fieldset></template>
      <p id="dialog-description" v-else-if="modalKind === 'format'">将 <strong>{{ activeDocument?.filename }}</strong> 切换为 <strong>{{ selectedDate }}.{{ targetFormat }}</strong>。<br />文本内容保持原样，保存后会更换文件扩展名。若目标文件已存在，将阻止切换。</p><p id="dialog-description" v-else-if="modalKind === 'restore'">将使用 AI 润色前保留的原始日报替换当前文本，并自动保存。</p><p id="dialog-description" v-else>当前未保存的编辑将被磁盘文件替换。<br />请先复制需要保留的草稿。</p><p v-if="error" class="error-text dialog-error" role="alert">{{ error }}</p><div class="dialog-actions"><md-text-button class="dialog-button dialog-button-secondary" :disabled="busy" @click="modal?.close()">取消</md-text-button><md-filled-button class="dialog-button dialog-button-primary" :disabled="busy" @click="confirmModal">{{ busy ? '处理中…' : modalKind === 'create' ? '确定新增' : modalKind === 'format' ? '确认切换' : modalKind === 'restore' ? '恢复原文' : '重新载入' }}</md-filled-button></div></dialog>
    <AiPolishDialog :visible="aiVisible" :generating="generating" :original="aiOriginal" :result="aiResult" :error="aiResultError" :provider="defaultProvider?.name ?? ''" @close="ai.close" @regenerate="ai.generate" @edit="aiResult = $event; aiResultError = ''" @apply="applyAi(aiResult)" @restore="applyAi(aiOriginal, true)" />
  </div>
</template>
