<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import {
  PhChatText,
  PhFolderOpen,
  PhHardDrives,
  PhSparkle,
  PhMonitor,
  PhMinus,
  PhPlus,
  PhFileXls,
  PhFileText,
  PhFloppyDisk,
  PhDatabase,
  PhCalendarCheck,
} from '@phosphor-icons/vue'
import { aiApi, type AiProvider, type DetectedCli } from '../lib/ai'
import type { Workspace } from '../lib/workspace'
import { useSettingsDraft, type WorkspaceSettings } from '../lib/settingsDraft'
import ModelPicker from './ModelPicker.vue'
import CliModelPicker from './CliModelPicker.vue'
import { APPEARANCE_SCALES, DEFAULT_APPEARANCE_SCALE } from '../lib/useAppearance'
import ExcelSettings from './ExcelSettings.vue'
import SummaryOutputSettings from './SummaryOutputSettings.vue'
import DatabaseSettings from './DatabaseSettings.vue'
import AppSelect from './AppSelect.vue'
import {
  clampHistoryLimit,
  MAX_HISTORY_LIMIT_BYTES,
  MIN_HISTORY_LIMIT_BYTES,
  type DatabaseTransferResult,
} from '../lib/database'
import HolidaySettings from './HolidaySettings.vue'
import type { HolidayState } from '../lib/holidays'

const props = defineProps<{
  workspace: Workspace | null
  picking: boolean
  bundle: WorkspaceSettings
  loading: boolean
  saving: boolean
  databaseBusy: boolean
  backupWarning: string
  backupRetrying: boolean
  beforeDatabaseTransfer: () => Promise<boolean>
  error: string
  save: (settings: WorkspaceSettings) => Promise<boolean>
  holidays: HolidayState
  holidayLoading: boolean
  holidaySyncing: boolean
  holidayError: string
  currentYear: number
}>()
const emit = defineEmits<{
  back: []
  workspace: []
  retryBackup: []
  databaseBusy: [value: boolean]
  databaseTransferred: [result: DatabaseTransferResult]
  syncHolidays: [year: number]
}>()
const {
  draft,
  unchanged,
  save: saveDraft,
  reset: resetDraft,
  saving: draftSaving,
  saved,
  error: draftError,
} = useSettingsDraft(
  () => props.bundle,
  (settings) => props.save(settings),
)
const busy = computed(() => props.loading || props.saving || props.databaseBusy || draftSaving.value)
const databaseForm = ref<InstanceType<typeof DatabaseSettings>>()
const excelForm = ref<InstanceType<typeof ExcelSettings>>()
const summaryForm = ref<InstanceType<typeof SummaryOutputSettings>>()
const validationError = ref('')
const historyLimitGb = computed(() => Math.round(draft.value.database.historyLimitBytes / (1024 * 1024 * 1024)))
const historyLimitMinGb = MIN_HISTORY_LIMIT_BYTES / (1024 * 1024 * 1024)
const historyLimitMaxGb = MAX_HISTORY_LIMIT_BYTES / (1024 * 1024 * 1024)
function onHistoryLimitInput(event: Event) {
  const value = Number((event.target as HTMLInputElement).value)
  draft.value.database.historyLimitBytes = clampHistoryLimit(
    Number.isFinite(value) ? value * 1024 * 1024 * 1024 : Number.NaN,
  )
}
const saveError = computed(() => validationError.value || draftError.value || props.error)
const aiReady = computed(() => {
  const provider = draft.value.ai.providers.find((item) => item.id === draft.value.ai.defaultProviderId)
  if (!provider || !provider.name.trim()) return false
  if (provider.kind !== 'http') return provider.executable.trim().startsWith('/')
  try {
    const url = new URL(provider.baseUrl)
    return !!provider.model.trim() && ['http:', 'https:'].includes(url.protocol) && !!url.hostname
  } catch {
    return false
  }
})
function validateAi(): boolean {
  const ai = draft.value.ai
  if (ai.generationPreference.length > 2000 || ai.summaryPrompt.length > 2000) {
    validationError.value = 'AI 提示词不能超过 2000 字。'
    navigate('prompts')
    return false
  }
  for (const provider of ai.providers) {
    let message = ''
    if (!provider.name.trim()) message = '请填写 AI 名称。'
    else if (provider.kind === 'http') {
      try {
        const url = new URL(provider.baseUrl)
        if (!['http:', 'https:'].includes(url.protocol) || !url.hostname) message = '请填写有效的 AI 接口地址。'
      } catch {
        message = '请填写有效的 AI 接口地址。'
      }
      if (!message && !provider.model.trim()) message = '请填写 AI 模型 ID。'
    } else if (!provider.executable.trim().startsWith('/')) message = '请填写 CLI 的绝对路径，或重新检测本机 CLI。'
    if (message) {
      editingId.value = provider.id
      validationError.value = message
      void navigate('ai', `settings-provider-${provider.id}`)
      return false
    }
  }
  return true
}
async function saveAll(): Promise<boolean> {
  if (props.loading || props.databaseBusy) return false
  if (draftSaving.value) return saveDraft()
  if (props.saving) return false
  validationError.value = ''
  if (!props.workspace) {
    validationError.value = '请先选择工作空间，再保存设置。'
    navigate('workspace')
    return false
  }
  if (databaseForm.value && !databaseForm.value.validate()) {
    validationError.value = '请检查数据库存储配置。'
    void navigate('database')
    return false
  }
  if (!validateAi()) return false
  if (summaryForm.value && !summaryForm.value.validate()) {
    validationError.value = '请检查总结输出目录。'
    navigate('summary')
    return false
  }
  if (excelForm.value && !excelForm.value.validate()) {
    validationError.value = '请检查 Excel 输出配置。'
    navigate('excel')
    return false
  }
  return saveDraft()
}
function reset() {
  resetDraft()
  validationError.value = ''
  localError.value = ''
  if (!draft.value.ai.providers.some((provider) => provider.id === editingId.value)) editingId.value = ''
}
watch(
  draft,
  () => {
    validationError.value = ''
  },
  { deep: true, flush: 'sync' },
)
defineExpose({ save: saveAll, unchanged, reset, navigate })
const detected = ref<DetectedCli[]>([])
const detecting = ref(false)
const detectedOnce = ref(false)
const localError = ref('')
const editingId = ref('')
const preset = ref('deepseek')
const presetOptions = [
  { value: 'deepseek', label: 'DeepSeek' },
  { value: 'gemini', label: 'Gemini' },
  { value: 'custom', label: '自定义兼容接口' },
]
const pageRoot = ref<HTMLElement>()
const contentHeading = ref<HTMLElement>()
type SettingsSection = 'workspace' | 'appearance' | 'holidays' | 'ai' | 'prompts' | 'summary' | 'excel' | 'database'
const navigation = [
  {
    label: '基础设置',
    items: [
      { id: 'workspace', title: '工作空间', description: '日报与配置的保存位置', icon: PhFolderOpen },
      { id: 'appearance', title: '外观与字体', description: '调整阅读时的字体大小', icon: PhMonitor },
      { id: 'holidays', title: '放假信息', description: '手动同步放假与调休安排', icon: PhCalendarCheck },
      { id: 'database', title: '数据存储', description: '数据库备份、复制与迁移', icon: PhDatabase },
    ],
  },
  {
    label: 'AI 助手',
    items: [
      { id: 'ai', title: '模型配置', description: '管理模型与默认 AI', icon: PhSparkle },
      { id: 'prompts', title: '生成提示词', description: '设置润色与总结规则', icon: PhChatText },
    ],
  },
  {
    label: '文件输出',
    items: [
      { id: 'summary', title: '总结文件', description: 'Markdown 与 TXT 导出目录', icon: PhFileText },
      { id: 'excel', title: 'Excel 周报', description: '模板、填充规则与输出目录', icon: PhFileXls },
    ],
  },
] satisfies {
  label: string
  items: { id: SettingsSection; title: string; description: string; icon: typeof PhFolderOpen }[]
}[]
const activeSection = ref<SettingsSection>('workspace')
const activeGroup = computed(() =>
  navigation.find((group) => group.items.some((item) => item.id === activeSection.value))!,
)
const activeItem = computed(() => activeGroup.value.items.find((item) => item.id === activeSection.value)!)
const sectionChanges = computed<Record<SettingsSection, boolean>>(() => {
  const current = draft.value
  const stored = props.bundle
  const differs = (left: unknown, right: unknown) => JSON.stringify(left) !== JSON.stringify(right)
  return {
    workspace: false,
    holidays: false,
    database: differs(current.database, stored.database),
    appearance: differs(current.appearance, stored.appearance),
    ai: differs(
      [current.ai.providers, current.ai.defaultProviderId],
      [stored.ai.providers, stored.ai.defaultProviderId],
    ),
    prompts: differs(
      [current.ai.generationPreference, current.ai.summaryPrompt],
      [stored.ai.generationPreference, stored.ai.summaryPrompt],
    ),
    summary: differs(current.summary, stored.summary),
    excel: differs(current.excel, stored.excel),
  }
})
const changedSections = computed(() => Object.values(sectionChanges.value).filter(Boolean).length)
async function navigate(section: SettingsSection, targetId?: string) {
  activeSection.value = section
  await nextTick()
  const scroller = pageRoot.value?.querySelector<HTMLElement>(`#settings-panel-${section}`)
  scroller?.scrollTo({ top: 0, behavior: 'instant' })
  if (document.querySelector('dialog[open]')) return
  const target = targetId ? document.getElementById(targetId) : undefined
  if (target && scroller?.contains(target)) {
    const top = scroller.scrollTop + target.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 24
    scroller.scrollTo({ top, behavior: 'instant' })
    target.focus({ preventScroll: true })
  } else contentHeading.value?.focus({ preventScroll: true })
}
function changeScale(step: -1 | 1) {
  const index = APPEARANCE_SCALES.findIndex((scale) => scale === draft.value.appearance.scale)
  const next = APPEARANCE_SCALES[index + step]
  if (!busy.value && next !== undefined) draft.value.appearance.scale = next
}
function onScaleKeydown(event: KeyboardEvent) {
  if (busy.value) return
  if (event.key === 'ArrowDown' || event.key === 'ArrowLeft') {
    event.preventDefault()
    changeScale(-1)
  } else if (event.key === 'ArrowUp' || event.key === 'ArrowRight') {
    event.preventDefault()
    changeScale(1)
  } else if (event.key === 'Home' || event.key === 'End') {
    event.preventDefault()
    draft.value.appearance.scale =
      event.key === 'Home' ? APPEARANCE_SCALES[0] : APPEARANCE_SCALES[APPEARANCE_SCALES.length - 1]
  }
}
async function detect() {
  detecting.value = true
  localError.value = ''
  try {
    detected.value = await aiApi.detect()
    detectedOnce.value = true
  } catch (cause) {
    localError.value = String(cause)
  } finally {
    detecting.value = false
  }
}
function addCli(cli: DetectedCli) {
  const existing = draft.value.ai.providers.find(
    (provider) => provider.kind === cli.kind && provider.executable === cli.executable,
  )
  if (existing) {
    editingId.value = existing.id
    return
  }
  const provider: AiProvider = {
    id: crypto.randomUUID(),
    name: cli.name,
    kind: cli.kind,
    executable: cli.executable,
    model: '',
    baseUrl: '',
    apiKey: '',
  }
  draft.value.ai.providers.push(provider)
  editingId.value = provider.id
}
function addCustom() {
  const provider: AiProvider = {
    id: crypto.randomUUID(),
    name: preset.value === 'deepseek' ? 'DeepSeek' : preset.value === 'gemini' ? 'Gemini' : '自定义 AI',
    kind: 'http',
    executable: '',
    model: preset.value === 'deepseek' ? 'deepseek-chat' : '',
    baseUrl:
      preset.value === 'deepseek'
        ? 'https://api.deepseek.com/v1'
        : preset.value === 'gemini'
          ? 'https://generativelanguage.googleapis.com/v1beta/openai'
          : '',
    apiKey: '',
  }
  draft.value.ai.providers.push(provider)
  editingId.value = provider.id
}
function remove(id: string) {
  draft.value.ai.providers = draft.value.ai.providers.filter((provider) => provider.id !== id)
  if (draft.value.ai.defaultProviderId === id) draft.value.ai.defaultProviderId = null
  if (editingId.value === id) editingId.value = ''
}
</script>

<template>
  <div class="settings-layout">
    <main ref="pageRoot" class="settings-page">
      <aside class="settings-sidebar">
        <div class="settings-sidebar-heading">
          <h1>设置</h1>
          <p>让记录更贴合你的工作习惯。</p>
        </div>
        <nav class="settings-nav" aria-label="设置分类">
          <div v-for="group in navigation" :key="group.label" class="settings-nav-group">
            <p class="settings-nav-group-title">{{ group.label }}</p>
            <button
              v-for="item in group.items"
              :key="item.id"
              type="button"
              class="settings-nav-item"
              :aria-current="activeSection === item.id ? 'page' : undefined"
              :aria-controls="`settings-panel-${item.id}`"
              @click="navigate(item.id)"
            >
              <span class="settings-nav-icon"><component :is="item.icon" :size="18" aria-hidden="true" /></span>
              <span class="settings-nav-copy"
                ><strong>{{ item.title }}</strong
                ><small>{{ item.description }}</small></span
              >
              <span
                v-if="sectionChanges[item.id]"
                class="settings-nav-dirty"
                role="img"
                aria-label="有未保存的修改"
              ></span>
            </button>
          </div>
        </nav>
        <div class="settings-sidebar-note">
          <PhHardDrives :size="17" aria-hidden="true" /><span
            ><strong>{{ workspace?.name ?? '尚未选择工作空间' }}</strong
            ><br />设置随工作空间保存</span
          >
        </div>
      </aside>
      <div class="settings-content">
        <header class="settings-content-heading">
          <span class="settings-category-label">{{ activeGroup.label }}</span>
          <h2 ref="contentHeading" tabindex="-1">{{ activeItem.title }}</h2>
          <p>{{ activeItem.description }}</p>
        </header>
        <div class="settings-panels">
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'workspace'"
              id="settings-panel-workspace"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'workspace'"
              aria-labelledby="workspace-settings-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="workspace-settings-title">当前工作空间</h3>
                  <p>每天的记录集中保存在这里。</p>
                </div>
                <button class="text-action" :disabled="picking || busy" @click="emit('workspace')">
                  <Transition name="button-label" mode="out-in"
                    ><span :key="String(picking)">{{ picking ? '正在打开…' : '切换工作空间' }}</span></Transition
                  >
                </button>
              </div>
              <div class="workspace-location">
                <PhFolderOpen :size="24" />
                <div>
                  <strong>{{ workspace?.name ?? '尚未选择工作空间' }}</strong
                  ><span>{{ workspace?.path ?? '请先选择一个本地文件夹' }}</span>
                </div>
              </div>
              <div class="settings-details-list">
                <div><strong>日报文件</strong><span>按年份归档，每天一份 Markdown 或 TXT 文件。</span></div>
                <div><strong>专属配置</strong><span>字体、AI 与输出设置保存在当前工作空间，切换时同步切换。</span></div>
              </div>
            </section>
          </Transition>
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'appearance'"
              id="settings-panel-appearance"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'appearance'"
              aria-labelledby="appearance-settings-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="appearance-settings-title">字体大小</h3>
                  <p>适应当前窗口，保留舒适的阅读比例。</p>
                </div>
              </div>
              <div class="appearance-setting-row">
                <div class="appearance-setting-copy">
                  <h4 id="appearance-scale-label">界面缩放</h4>
                  <p id="appearance-scale-description">仅调整字体大小，保存后应用到整个 App。</p>
                  <span class="appearance-scale-range">80%–120% · 每次 10%</span>
                </div>
                <div class="appearance-scale-actions">
                  <div
                    class="appearance-stepper"
                    role="group"
                    aria-labelledby="appearance-scale-label"
                    aria-describedby="appearance-scale-description"
                    @keydown="onScaleKeydown"
                  >
                    <button
                      type="button"
                      aria-label="缩小字体"
                      title="缩小字体"
                      :disabled="busy || draft.appearance.scale <= APPEARANCE_SCALES[0]"
                      @click="changeScale(-1)"
                    >
                      <PhMinus :size="18" aria-hidden="true" />
                    </button>
                    <output class="appearance-scale-value" aria-live="polite" aria-atomic="true"
                      ><Transition name="button-label" mode="out-in"
                        ><span :key="draft.appearance.scale">{{ draft.appearance.scale }}%</span></Transition
                      ></output
                    >
                    <button
                      type="button"
                      aria-label="放大字体"
                      title="放大字体"
                      :disabled="busy || draft.appearance.scale >= APPEARANCE_SCALES[APPEARANCE_SCALES.length - 1]"
                      @click="changeScale(1)"
                    >
                      <PhPlus :size="18" aria-hidden="true" />
                    </button>
                  </div>
                  <button
                    type="button"
                    class="text-action appearance-reset"
                    :disabled="busy || draft.appearance.scale === DEFAULT_APPEARANCE_SCALE"
                    @click="draft.appearance.scale = DEFAULT_APPEARANCE_SCALE"
                  >
                    恢复默认
                  </button>
                </div>
              </div>
              <div
                class="appearance-preview"
                :style="{ '--preview-font-unit': `calc(var(--fluid-font-unit) * ${draft.appearance.scale / 100})` }"
              >
                <span>字体预览 · {{ draft.appearance.scale }}%</span>
                <strong>每一天，一份工作记录</strong>
                <p>记录今天的进展，让明天的工作更从容。</p>
              </div>
            </section>
          </Transition>
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'holidays'"
              id="settings-panel-holidays"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'holidays'"
              aria-labelledby="holiday-settings-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="holiday-settings-title">放假与调休安排</h3>
                  <p>按需同步，让日历显示最新的年度安排。</p>
                </div>
              </div>
              <HolidaySettings
                :state="holidays"
                :current-year="currentYear"
                :loading="holidayLoading"
                :syncing="holidaySyncing"
                :error="holidayError"
                @sync="emit('syncHolidays', $event)"
              />
            </section>
          </Transition>
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'database'"
              id="settings-panel-database"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'database'"
              aria-labelledby="database-settings-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="database-settings-title">日报存储</h3>
                  <p>设置新日报的默认格式，并按需选择文件、数据库或双存储。</p>
                </div>
              </div>
              <div class="database-field database-default-format">
                <span id="default-format-label" class="database-field-label">新日报默认格式</span>
                <div class="database-mode-options" role="radiogroup" aria-labelledby="default-format-label">
                  <label :class="{ selected: draft.database.defaultFormat === 'md' }" class="database-mode-label"
                    ><input
                      type="radio"
                      name="default-entry-format"
                      value="md"
                      :checked="draft.database.defaultFormat === 'md'"
                      :disabled="busy"
                      @change="draft.database.defaultFormat = 'md'"
                    /><span class="database-mode-copy"
                      ><strong>Markdown（.md）</strong><span>支持标题、列表与加粗，适合结构化日报。</span></span
                    ></label
                  ><label :class="{ selected: draft.database.defaultFormat === 'txt' }" class="database-mode-label"
                    ><input
                      type="radio"
                      name="default-entry-format"
                      value="txt"
                      :checked="draft.database.defaultFormat === 'txt'"
                      :disabled="busy"
                      @change="draft.database.defaultFormat = 'txt'"
                    /><span class="database-mode-copy"
                      ><strong>纯文本（.txt）</strong><span>只保留纯文本，适合直接粘贴或打印。</span></span
                    ></label
                  >
                </div>
                <p class="field-hint">只影响之后新建的日报；已有日报保持原格式，可在编辑区单独切换。</p>
              </div>
              <div class="database-field">
                <label class="database-field-label" for="history-limit">历史版本存储上限</label>
                <div class="history-limit">
                  <input
                    id="history-limit"
                    type="range"
                    :min="historyLimitMinGb"
                    :max="historyLimitMaxGb"
                    step="1"
                    :value="historyLimitGb"
                    :disabled="busy"
                    :aria-valuetext="`${historyLimitGb} GB`"
                    @input="onHistoryLimitInput"
                  /><output for="history-limit">{{ historyLimitGb }} GB</output>
                </div>
                <p class="field-hint">
                  默认 1 GB，滑块每格 1 GB（最大 10 GB）。历史版本超过该容量时，从最旧的一天开始整天清理；单日最多保留
                  100 个版本。
                </p>
              </div>
              <div v-if="backupWarning" class="database-backup-warning database-status settings-error" role="status">
                <p>{{ backupWarning }}</p>
                <button type="button" class="text-action" :disabled="busy || !unchanged" @click="emit('retryBackup')">
                  {{ backupRetrying ? '正在重试备份…' : '重试数据库备份' }}</button
                ><span v-if="!unchanged" class="field-hint">请先保存连接设置，再重试。</span>
              </div>
              <DatabaseSettings
                ref="databaseForm"
                v-model="draft.database"
                :busy="busy"
                :workspace-path="workspace?.path ?? ''"
                :unchanged="unchanged"
                :before-transfer="beforeDatabaseTransfer"
                @busy="emit('databaseBusy', $event)"
                @transferred="emit('databaseTransferred', $event)"
              />
            </section>
          </Transition>
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'ai'"
              id="settings-panel-ai"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'ai'"
              aria-labelledby="ai-settings-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="ai-settings-title">可用模型</h3>
                  <p>选择一个默认 AI，用于润色、总结和 Excel 填充。</p>
                </div>
                <button class="text-action" :disabled="detecting || busy" @click="detect">
                  <Transition name="button-label" mode="out-in"
                    ><span :key="String(detecting)">{{ detecting ? '正在检测…' : '检测本机 CLI' }}</span></Transition
                  >
                </button>
              </div>
              <Transition name="settings-reveal">
                <div v-if="detectedOnce" class="settings-reveal-shell">
                  <div class="settings-reveal-inner">
                    <div class="detected-clis">
                      <p v-if="!detected.length" class="settings-muted">
                        未检测到 Codex、Claude Code 或 OpenCode。可先安装并登录 CLI，或在下方添加模型接口。
                      </p>
                      <article v-for="cli in detected" :key="cli.executable" class="detected-row">
                        <div>
                          <strong
                            >{{ cli.name }}<small>{{ cli.version }}</small></strong
                          ><span>{{ cli.executable }}</span>
                        </div>
                        <button class="text-action" :disabled="busy" @click="addCli(cli)">
                          <Transition name="button-label" mode="out-in"
                            ><span
                              :key="
                                String(draft.ai.providers.some((provider) => provider.executable === cli.executable))
                              "
                              >{{
                                draft.ai.providers.some((provider) => provider.executable === cli.executable)
                                  ? '已添加'
                                  : '添加'
                              }}</span
                            ></Transition
                          >
                        </button>
                      </article>
                    </div>
                  </div>
                </div>
              </Transition>
              <div class="custom-ai-add">
                <label for="ai-preset">添加模型接口</label
                ><AppSelect id="ai-preset" v-model="preset" :options="presetOptions" :disabled="busy" /><button
                  class="text-action"
                  :disabled="busy"
                  @click="addCustom"
                >
                  添加
                </button>
              </div>
              <Transition name="status-fade"
                ><p v-if="!draft.ai.providers.length" class="settings-empty">
                  尚未配置 AI。检测本机 CLI，或添加一个模型接口。
                </p></Transition
              >
              <TransitionGroup name="provider-list" tag="div" class="provider-list">
                <article
                  v-for="provider in draft.ai.providers"
                  :id="`settings-provider-${provider.id}`"
                  :key="provider.id"
                  tabindex="-1"
                  class="provider-card"
                  :class="{ 'provider-default': draft.ai.defaultProviderId === provider.id }"
                >
                  <div class="provider-heading">
                    <label class="provider-choice"
                      ><input
                        :disabled="busy"
                        :checked="draft.ai.defaultProviderId === provider.id"
                        type="radio"
                        name="default-ai"
                        :value="provider.id"
                        @change="draft.ai.defaultProviderId = provider.id"
                      /><span
                        >{{ provider.name || '未命名 AI'
                        }}<small>{{
                          provider.kind === 'http' ? provider.model || '模型接口' : '本机 CLI'
                        }}</small></span
                      ></label
                    >
                    <div class="provider-actions">
                      <Transition name="status-fade"
                        ><span v-if="draft.ai.defaultProviderId === provider.id" class="default-label"
                          >默认 AI</span
                        ></Transition
                      ><button
                        class="text-action"
                        :aria-expanded="editingId === provider.id"
                        :disabled="busy"
                        @click="editingId = editingId === provider.id ? '' : provider.id"
                      >
                        <Transition name="button-label" mode="out-in"
                          ><span :key="String(editingId === provider.id)">{{
                            editingId === provider.id ? '收起' : '编辑'
                          }}</span></Transition
                        ></button
                      ><button class="text-action muted-action" :disabled="busy" @click="remove(provider.id)">
                        移除
                      </button>
                    </div>
                  </div>
                  <Transition name="settings-reveal">
                    <div
                      v-show="editingId === provider.id"
                      class="settings-reveal-shell"
                      :inert="editingId !== provider.id"
                    >
                      <div class="settings-reveal-inner">
                        <div class="provider-fields">
                          <label
                            >名称<input
                              v-model="provider.name"
                              :disabled="busy"
                              type="text"
                              placeholder="为这项 AI 起个名字"
                          /></label>
                          <template v-if="provider.kind === 'http'"
                            ><label
                              >接口地址<input
                                v-model="provider.baseUrl"
                                :disabled="busy"
                                type="url"
                                placeholder="https://example.com/v1"
                                spellcheck="false" /></label
                            ><label
                              >API Key<input
                                v-model="provider.apiKey"
                                :disabled="busy"
                                type="password"
                                placeholder="填写 API Key，本地服务可留空"
                                autocomplete="off"
                                spellcheck="false" /></label
                            ><ModelPicker
                              v-model="provider.model"
                              :base-url="provider.baseUrl"
                              :api-key="provider.apiKey"
                              :disabled="busy"
                            />
                            <p class="field-hint">
                              支持 Chat Completions 兼容接口，密钥保存在本机应用配置中。
                            </p></template
                          >
                          <template v-else
                            ><label
                              >CLI 路径<input
                                v-model="provider.executable"
                                :disabled="busy"
                                type="text"
                                spellcheck="false" /></label
                            ><CliModelPicker
                              v-model="provider.model"
                              :kind="provider.kind"
                              :executable="provider.executable"
                              :disabled="busy"
                            />
                            <p class="field-hint">
                              使用 CLI 已有的登录状态。首次使用前请在终端完成登录。本机 CLI
                              每次调用都要启动子进程，通常比模型接口慢，请预留等待时间。
                            </p></template
                          >
                        </div>
                      </div>
                    </div>
                  </Transition>
                </article>
              </TransitionGroup>
              <Transition name="status-fade"
                ><div v-if="draft.ai.providers.length" class="default-ai-control">
                  <button
                    class="text-action muted-action"
                    :disabled="busy || !draft.ai.defaultProviderId"
                    @click="draft.ai.defaultProviderId = null"
                  >
                    暂不设置默认 AI</button
                  ><Transition name="status-fade"
                    ><span v-if="!draft.ai.defaultProviderId">尚未选择默认 AI</span></Transition
                  >
                </div></Transition
              >
              <p v-if="!workspace" class="settings-muted workspace-settings-note">
                尚未选择工作空间，AI 设置暂时无法保存。请先在“工作空间”中选择文件夹。
              </p>
              <div class="ai-settings-footer">
                <span
                  >AI 润色与 AI 总结会向默认 AI 发送相关日报；Excel 输出经确认后，会发送所选日报、模板信息和 Excel
                  输出提示词。</span
                >
              </div>
              <Transition name="status-fade"
                ><p v-if="localError" class="settings-error" role="alert">{{ localError }}</p></Transition
              >
            </section>
          </Transition>
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'prompts'"
              id="settings-panel-prompts"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'prompts'"
              aria-labelledby="ai-prompts-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="ai-prompts-title">生成规则</h3>
                  <p>留空时使用默认规则，对所有 AI 模型生效。</p>
                </div>
              </div>
              <div class="generation-preference">
                <label for="polish-prompt">日报润色提示词</label>
                <p id="polish-prompt-hint">追加在固定的事实、安全和格式规则之后；留空时使用默认润色规则。</p>
                <textarea
                  id="polish-prompt"
                  v-model="draft.ai.generationPreference"
                  :disabled="busy"
                  maxlength="2000"
                  rows="4"
                  aria-describedby="polish-prompt-hint"
                  placeholder="例如：简洁客观，突出已完成事项和下一步计划，保留原有条目结构。"
                  spellcheck="false"
                ></textarea
                ><span class="preference-count">{{ draft.ai.generationPreference.length }} / 2000</span>
              </div>
              <div class="generation-preference">
                <label for="summary-prompt">框选总结提示词</label>
                <p id="summary-prompt-hint">用于对选中日期内的日报生成总结；留空时使用默认总结规则。</p>
                <textarea
                  id="summary-prompt"
                  v-model="draft.ai.summaryPrompt"
                  :disabled="busy"
                  maxlength="2000"
                  rows="4"
                  aria-describedby="summary-prompt-hint"
                  placeholder="例如：按项目归类，提炼成果、风险和下一步计划，控制在 500 字内。"
                  spellcheck="false"
                ></textarea
                ><span class="preference-count">{{ draft.ai.summaryPrompt.length }} / 2000</span>
              </div>
              <p class="field-hint prompt-note">
                提示词会追加在固定的安全与格式规则之后，对所有 AI 生效。日报内容始终作为待处理数据，不会被当作指令。
              </p>
              <p class="field-hint">提示词只在本机 AI 请求时使用，不会写入日报。</p>
            </section>
          </Transition>
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'summary'"
              id="settings-panel-summary"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'summary'"
              aria-labelledby="summary-output-settings-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="summary-output-settings-title">导出位置</h3>
                  <p>Markdown 和 TXT 格式共用一个输出目录。</p>
                </div>
              </div>
              <SummaryOutputSettings
                ref="summaryForm"
                v-model="draft.summary"
                :workspace-path="workspace?.path ?? ''"
                :busy="busy"
              />
            </section>
          </Transition>
          <Transition name="settings-panel">
            <section
              v-show="activeSection === 'excel'"
              id="settings-panel-excel"
              tabindex="0"
              class="settings-section settings-panel"
              :inert="activeSection !== 'excel'"
              aria-labelledby="excel-settings-title"
            >
              <div class="settings-section-heading">
                <div>
                  <h3 id="excel-settings-title">模板填充</h3>
                  <p>使用默认 AI，将所选日报整理为 Excel 周报。</p>
                </div>
              </div>
              <ExcelSettings
                ref="excelForm"
                v-model="draft.excel"
                :busy="busy"
                :ai-ready="aiReady"
                :workspace-path="workspace?.path ?? ''"
                @configure-ai="navigate('ai')"
              />
            </section>
          </Transition>
        </div>
      </div>
    </main>
    <footer class="settings-savebar" aria-label="保存全部设置">
      <div class="settings-savebar-inner">
        <div
          class="settings-save-status"
          :class="{ 'has-changes': !unchanged, 'has-error': !!saveError }"
          aria-live="polite"
          aria-atomic="true"
        >
          <p v-if="saveError" class="settings-save-error" role="alert">{{ saveError }}</p>
          <p v-else-if="loading">正在读取设置…</p>
          <p v-else-if="databaseBusy">正在处理数据库操作…</p>
          <p v-else-if="busy">正在保存全部设置…</p>
          <p v-else-if="!unchanged">{{ changedSections }} 个设置分类待保存</p>
          <p v-else>{{ saved ? '设置已保存' : '所有修改已保存' }}</p>
          <span>{{ unchanged ? '修改设置后，在这里统一保存。' : '切换分类会保留修改，点击保存后统一生效。' }}</span>
        </div>
        <button type="button" class="settings-save-button" :disabled="busy || unchanged || !workspace" @click="saveAll">
          <PhFloppyDisk :size="18" aria-hidden="true" /><Transition name="button-label" mode="out-in"
            ><span :key="String(busy)">{{ busy ? '正在保存…' : '保存设置' }}</span></Transition
          >
        </button>
      </div>
    </footer>
  </div>
</template>
