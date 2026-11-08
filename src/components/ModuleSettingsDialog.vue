<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, useId, watch } from 'vue'
import { PhGearSix, PhSlidersHorizontal, PhX } from '@phosphor-icons/vue'
import { moduleById } from '../modules/registry'
import { MODULE_SETTINGS_CONTEXT, type ModuleSettingsContext } from '../modules/settingsContext'
import type { ModuleType } from '../modules/types'
import type { Workspace } from '../lib/workspace'
import { useSettingsDraft, type WorkspaceSettings } from '../lib/settingsDraft'
import { aiSettingsReady } from '../lib/ai'
import type { HolidayState } from '../lib/holidays'
import type { GlobalSettingsSection } from '../lib/settingsSections'
import { createSurfaceMotion } from '../lib/surfaceMotion'
import type { WindowMotionRect } from '../lib/windowMotion'

const props = defineProps<{
  visible: boolean
  /** 当前打开的模块；没有模块时弹窗为空。 */
  module: ModuleType | null
  buttonRect?: () => WindowMotionRect | null
  workspace: Workspace | null
  /** 打开弹窗时的已保存设置，作为草稿基线。 */
  bundle: WorkspaceSettings
  /** 全局忙碌状态：读取设置、数据库操作等。 */
  busy: boolean
  /** 保存失败等外部错误信息。 */
  error: string
  save: (settings: WorkspaceSettings) => Promise<boolean>
  holidays: HolidayState
  holidayLoading: boolean
  holidaySyncing: boolean
  holidayError: string
  currentYear: number
}>()
const emit = defineEmits<{
  close: []
  'open-global-settings': [section: GlobalSettingsSection]
  'sync-holidays': [year: number]
}>()

// 可配置模块导航列表
const MODULE_NAV_ITEMS = [
  { id: 'editor' as ModuleType, label: '编辑模块', desc: '格式、预设骨架与润色' },
  { id: 'git' as ModuleType, label: '本地 Git', desc: '仓库、提交风格与凭据' },
  { id: 'excel' as ModuleType, label: 'Excel 报告', desc: '导出配置、模板与提示词' },
  { id: 'summary' as ModuleType, label: '智能总结', desc: '输出位置与提示词预设' },
  { id: 'calendar' as ModuleType, label: '日历设置', desc: '放假与调休同步' },
  { id: 'history' as ModuleType, label: '历史版本', desc: '快照与存储限制' },
]

// 各模块内部小节快捷锚点
const MODULE_SECTIONS: Record<string, Array<{ id: string; label: string }>> = {
  editor: [
    { id: 'editor-format-title', label: '默认格式' },
    { id: 'editor-files-title', label: '保存位置' },
    { id: 'editor-skeleton-title', label: '每日骨架' },
    { id: 'editor-prompt-title', label: '日报润色' },
  ],
  git: [
    { id: 'git-repositories-heading', label: '仓库管理' },
    { id: 'git-prompt-heading', label: '提交风格' },
    { id: 'github-authentication-heading', label: 'GitHub 账号' },
    { id: 'saved-git-heading', label: '凭据管理' },
  ],
  excel: [
    { id: 'excel-settings-title', label: '导出配置' },
    { id: 'excel-prompt-title', label: '提示词' },
  ],
  summary: [
    { id: 'summary-output-title', label: '输出位置' },
    { id: 'summary-prompt-title', label: '提示词预设' },
  ],
  calendar: [{ id: 'calendar-sync-title', label: '放假调休' }],
  history: [{ id: 'history-limit-title', label: '存储上限' }],
}

// 父组件关闭时会清空 module；收缩结束前保留标题、表单及布局。
const displayedModule = ref(props.module)
const moduleDefinition = computed(() => (displayedModule.value ? moduleById(displayedModule.value) : undefined))
const definition = computed(() => moduleDefinition.value?.settings)
const savable = computed(() => definition.value?.savable !== false)
const titleId = useId()
const panel = ref<HTMLElement>()
const backdrop = ref<HTMLElement>()
const closeButton = ref<HTMLButtonElement>()
const body = ref<HTMLElement>()
const content = ref<{ validate?: () => boolean; focusInvalid?: () => void }>()
const localError = ref('')
type CloseTarget = 'settings' | 'close'
type PendingAction = { kind: 'close'; target: CloseTarget } | { kind: 'global'; section: GlobalSettingsSection }
const pendingAction = ref<PendingAction | null>(null)
let returnFocus: HTMLElement | null = null
let closeTarget: CloseTarget = 'settings'
const motion = createSurfaceMotion({
  openingRect: () => props.buttonRect?.(),
  closingRect: () => (closeTarget === 'close' ? closeButton.value?.getBoundingClientRect() : props.buttonRect?.()),
  backdrop: () => backdrop.value,
})
function enter(_element: Element, done: () => void) {
  if (panel.value) motion.enter(panel.value, done)
  else done()
}
function leave(_element: Element, done: () => void) {
  if (panel.value) motion.leave(panel.value, done)
  else done()
}
function afterLeave() {
  if (props.visible) return
  displayedModule.value = null
  pendingAction.value = null
  localError.value = ''
  const focused = document.activeElement
  if (
    !document.querySelector('dialog[open]:not(.is-inline)') &&
    returnFocus?.isConnected &&
    (focused === document.body || (focused && panel.value?.contains(focused)))
  )
    returnFocus.focus({ preventScroll: true })
  returnFocus = null
}

const {
  draft,
  unchanged,
  save: saveDraft,
  reset: resetDraft,
  saving,
  error: draftError,
} = useSettingsDraft(
  () => props.bundle,
  (settings) => props.save(settings),
)

const moduleDirty = computed<Record<ModuleType, boolean>>(() => {
  const current = draft.value
  const baseline = props.bundle
  const differs = (a: unknown, b: unknown) => JSON.stringify(a) !== JSON.stringify(b)

  return {
    editor:
      differs(current.editor, baseline.editor) ||
      differs(current.files?.journal, baseline.files?.journal) ||
      current.database.defaultFormat !== baseline.database.defaultFormat ||
      current.ai.generationPreference !== baseline.ai.generationPreference,
    git: differs(current.git, baseline.git),
    excel: differs(current.excel, baseline.excel) || differs(current.files?.excel, baseline.files?.excel),
    summary: differs(current.summary, baseline.summary) || differs(current.files?.summary, baseline.files?.summary),
    calendar: false,
    history: current.database.historyLimitBytes !== baseline.database.historyLimitBytes,
    overview: false,
    directory: false,
  }
})

function selectModule(id: ModuleType) {
  if (displayedModule.value === id) return
  displayedModule.value = id
  localError.value = ''
  void nextTick(() => {
    body.value?.scrollTo({ top: 0, behavior: 'instant' })
  })
}

function scrollToSection(targetId: string) {
  void nextTick(() => {
    const el = body.value?.querySelector(`[heading-id="${targetId}"], #${targetId}, .${targetId}`)
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'start' })
    }
  })
}

const busy = computed(() => props.busy || saving.value)
const statusError = computed(() => props.error || localError.value || draftError.value)
const aiReady = computed(() => aiSettingsReady(draft.value.ai))

function requestGlobalSettings(section: GlobalSettingsSection) {
  if (busy.value) return
  closeTarget = 'settings'
  if (!savable.value || unchanged.value) {
    emit('open-global-settings', section)
    return
  }
  pendingAction.value = { kind: 'global', section }
}
function requestClose(target: CloseTarget = 'settings') {
  if (!props.visible || busy.value) return
  closeTarget = target
  if (!savable.value || unchanged.value) {
    emit('close')
    return
  }
  pendingAction.value = { kind: 'close', target }
}
function cancelPending() {
  pendingAction.value = null
}
function confirmDiscard() {
  const action = pendingAction.value
  if (!action) return
  if (action.kind === 'global') emit('open-global-settings', action.section)
  else {
    closeTarget = action.target
    emit('close')
  }
}
async function saveSettings() {
  if (!savable.value || busy.value || unchanged.value) return
  localError.value = ''
  const valid = content.value?.validate?.() ?? true
  if (!valid) {
    localError.value = '请检查标出的设置项后重试。'
    await nextTick()
    content.value?.focusInvalid?.()
    return
  }
  if (await saveDraft()) {
    closeTarget = 'settings'
    emit('close')
  }
}
function onKeydown(event: KeyboardEvent) {
  if (!props.visible || event.key !== 'Escape' || event.defaultPrevented) return
  // 原生 <dialog> 自己处理 Esc；这里只关闭自己的浮层。
  if (document.querySelector('dialog[open]:not(.is-inline)')) return
  event.preventDefault()
  event.stopPropagation()
  if (pendingAction.value) cancelPending()
  else requestClose()
}
watch(
  () => props.module,
  (module) => {
    if (module) {
      displayedModule.value = module
      if (props.visible) resetDraft()
    }
  },
  { flush: 'sync' },
)
watch(
  () => props.visible,
  async (visible) => {
    if (visible) {
      closeTarget = 'settings'
      pendingAction.value = null
      localError.value = ''
      displayedModule.value = props.module ?? 'editor'
      if (!definition.value) {
        emit('close')
        return
      }
      resetDraft()
      const focused = document.activeElement
      if (focused instanceof HTMLElement && !panel.value?.contains(focused)) returnFocus = focused
      await nextTick()
      body.value?.scrollTo({ top: 0 })
      if (props.visible) panel.value?.focus({ preventScroll: true })
    }
  },
  { immediate: true },
)

provide<ModuleSettingsContext>(MODULE_SETTINGS_CONTEXT, {
  draft,
  workspace: computed(() => props.workspace),
  busy,
  aiReady,
  holidays: {
    state: computed(() => props.holidays),
    currentYear: computed(() => props.currentYear),
    loading: computed(() => props.holidayLoading),
    syncing: computed(() => props.holidaySyncing),
    error: computed(() => props.holidayError),
  },
  openGlobalSettings: requestGlobalSettings,
  syncHolidays: (year: number) => emit('sync-holidays', year),
})

onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  motion.dispose()
})

defineExpose({ requestClose, unchanged, isOpen: () => props.visible })
</script>

<template>
  <Transition
    :css="false"
    appear
    @enter="enter"
    @leave="leave"
    @after-leave="afterLeave"
    @enter-cancelled="motion.cancel()"
    @leave-cancelled="motion.cancel()"
  >
    <div v-show="visible" class="module-settings-layer" :inert="!visible">
      <div ref="backdrop" class="module-settings-backdrop" aria-hidden="true"></div>
      <section
        ref="panel"
        class="module-settings-dialog"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        :aria-describedby="definition?.description ? `${titleId}-description` : undefined"
        tabindex="-1"
      >
        <!-- 左侧导航侧栏 -->
        <aside class="module-settings-sidebar">
          <div class="module-settings-sidebar-header">
            <span class="module-settings-sidebar-icon">
              <PhGearSix :size="16" aria-hidden="true" />
            </span>
            <span class="module-settings-sidebar-title">模块设置</span>
          </div>

          <nav class="module-settings-nav" aria-label="模块设置分类">
            <div v-for="item in MODULE_NAV_ITEMS" :key="item.id" class="module-settings-nav-group">
              <button
                type="button"
                class="module-settings-nav-item"
                :class="{ 'is-active': displayedModule === item.id }"
                :aria-current="displayedModule === item.id ? 'page' : undefined"
                @click="selectModule(item.id)"
              >
                <span class="module-settings-nav-icon">
                  <component :is="moduleById(item.id)?.icon ?? PhGearSix" :size="16" aria-hidden="true" />
                </span>
                <span class="module-settings-nav-copy">
                  <strong>{{ item.label }}</strong>
                  <small>{{ item.desc }}</small>
                </span>
                <span
                  v-if="moduleDirty[item.id]"
                  class="module-settings-dirty-dot"
                  role="img"
                  aria-label="有未保存修改"
                  title="有未保存修改"
                ></span>
              </button>

              <div
                v-if="displayedModule === item.id && MODULE_SECTIONS[item.id]?.length"
                class="module-settings-subnav"
              >
                <button
                  v-for="sec in MODULE_SECTIONS[item.id]"
                  :key="sec.id"
                  type="button"
                  class="module-settings-subnav-item"
                  @click="scrollToSection(sec.id)"
                >
                  <span>{{ sec.label }}</span>
                </button>
              </div>
            </div>
          </nav>

          <div class="module-settings-sidebar-footer">
            <button
              type="button"
              class="module-settings-global-link"
              title="前往全局偏好设置（工作空间、外观与模型配置）"
              @click="requestGlobalSettings('workspace')"
            >
              <PhSlidersHorizontal :size="14" aria-hidden="true" />
              <span>全局偏好设置</span>
            </button>
          </div>
        </aside>

        <!-- 右侧设置主体区 -->
        <div class="module-settings-main">
          <header class="module-settings-heading">
            <span class="module-settings-icon">
              <component :is="moduleDefinition?.icon ?? PhGearSix" :size="21" aria-hidden="true" />
            </span>
            <div class="module-settings-heading-copy">
              <h2 :id="titleId">{{ definition?.title ?? '模块设置' }}</h2>
              <p v-if="definition?.description" :id="`${titleId}-description`" class="module-settings-description">
                {{ definition.description }}
              </p>
            </div>
            <button
              ref="closeButton"
              type="button"
              class="module-settings-close"
              aria-label="关闭模块设置"
              title="关闭（Esc）"
              :disabled="busy"
              @click="requestClose('close')"
            >
              <PhX :size="17" aria-hidden="true" />
            </button>
          </header>

          <div ref="body" class="module-settings-body">
            <component :is="definition?.component" v-if="definition" ref="content" />
          </div>

          <footer class="module-settings-footer">
            <div
              class="module-settings-status"
              :class="{ 'has-changes': (savable || !unchanged) && !unchanged, 'has-error': !!statusError }"
              aria-live="polite"
              aria-atomic="true"
            >
              <p v-if="statusError" class="settings-error" role="alert">{{ statusError }}</p>
              <p v-else-if="saving">正在保存设置…</p>
              <p v-else-if="busy">正在读取设置…</p>
              <p v-else-if="!savable && unchanged">同步后立即生效，无需保存。</p>
              <p v-else-if="!unchanged">有未保存的修改</p>
              <p v-else>所有修改已保存</p>
            </div>
            <div v-if="pendingAction" class="module-settings-confirm">
              <span>{{
                pendingAction.kind === 'global'
                  ? '有未保存的修改，前往全局设置将丢失这些修改。'
                  : '关闭后将丢失这些修改。'
              }}</span>
              <button type="button" class="text-action" @click="cancelPending">继续编辑</button>
              <button type="button" class="settings-save-button" @click="confirmDiscard">放弃修改</button>
            </div>
            <div v-else class="module-settings-actions">
              <button v-if="savable || !unchanged" type="button" class="text-action" @click="requestClose()">
                {{ unchanged ? '关闭' : '取消' }}
              </button>
              <button
                v-if="savable || !unchanged"
                type="button"
                class="settings-save-button module-settings-save"
                :disabled="busy || unchanged"
                @click="saveSettings"
              >
                {{ saving ? '正在保存…' : '保存设置' }}
              </button>
              <button v-else type="button" class="settings-save-button module-settings-save" @click="requestClose()">
                完成
              </button>
            </div>
          </footer>
        </div>
      </section>
    </div>
  </Transition>
</template>
