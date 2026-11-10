<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, useId, watch, type Component } from 'vue'
import {
  PhCalendarDots,
  PhChatTeardropText,
  PhClockCounterClockwise,
  PhCpu,
  PhFileText,
  PhFileXls,
  PhFolder,
  PhGearSix,
  PhGitBranch,
  PhKey,
  PhLayout,
  PhSliders,
  PhSlidersHorizontal,
  PhSparkle,
  PhX,
} from '@phosphor-icons/vue'
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
  initialSectionId?: string | null
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

export interface ModuleSectionItem {
  id: string
  title: string
  desc: string
  icon: Component
  targetId: string
}

// 保持各模块自身的设置选项列表
const MODULE_SECTIONS_CONFIG: Record<string, ModuleSectionItem[]> = {
  editor: [
    {
      id: 'editor-format',
      title: '默认格式',
      desc: 'Markdown 与纯文本格式',
      icon: PhFileText,
      targetId: 'editor-format-title',
    },
    {
      id: 'editor-files',
      title: '保存位置',
      desc: '保存目录与命名规则',
      icon: PhFolder,
      targetId: 'editor-files-title',
    },
    {
      id: 'editor-skeleton',
      title: '每日骨架',
      desc: '空白日报模板与骨架',
      icon: PhLayout,
      targetId: 'editor-skeleton-title',
    },
    {
      id: 'editor-prompt',
      title: '日报润色',
      desc: 'AI 润色提示词及模型',
      icon: PhSparkle,
      targetId: 'editor-prompt-title',
    },
  ],
  git: [
    {
      id: 'git-repositories',
      title: '本地仓库',
      desc: '添加与管理项目仓库',
      icon: PhGitBranch,
      targetId: 'git-repositories-heading',
    },
    {
      id: 'git-prompt',
      title: '提交说明',
      desc: 'AI 提交说明与风格预设',
      icon: PhSparkle,
      targetId: 'prompt-title',
    },
    {
      id: 'git-credentials',
      title: '认证凭据',
      desc: 'GitHub 账号与本机凭据',
      icon: PhKey,
      targetId: 'git-auth-heading',
    },
    {
      id: 'git-advanced',
      title: '高级设置',
      desc: 'Git 程序执行路径',
      icon: PhSliders,
      targetId: 'git-advanced-heading',
    },
  ],
  excel: [
    {
      id: 'excel-export',
      title: 'Excel 输出',
      desc: '生成配置、模板与提示词',
      icon: PhFileXls,
      targetId: 'excel-output-title',
    },
    {
      id: 'excel-files',
      title: '保存位置',
      desc: '导出目录与命名规则',
      icon: PhFolder,
      targetId: 'excel-directory-title',
    },
  ],
  summary: [
    {
      id: 'summary-ai',
      title: '默认 AI',
      desc: '全局模型与服务状态',
      icon: PhCpu,
      targetId: 'summary-default-ai-title',
    },
    {
      id: 'summary-prompt',
      title: '总结提示词',
      desc: '多方案提示词与系统设定',
      icon: PhChatTeardropText,
      targetId: 'summary-prompt-title',
    },
    {
      id: 'summary-files',
      title: '保存位置',
      desc: '总结保存目录与命名规则',
      icon: PhFolder,
      targetId: 'summary-directory-title',
    },
  ],
  calendar: [
    {
      id: 'calendar-sync',
      title: '放假与调休',
      desc: '法定节假日与调休安排',
      icon: PhCalendarDots,
      targetId: 'calendar-sync-title',
    },
  ],
  history: [
    {
      id: 'history-limit',
      title: '存储上限',
      desc: '历史版本空间限制',
      icon: PhClockCounterClockwise,
      targetId: 'history-limit-title',
    },
  ],
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

// 当前模块自身拥有的设置选项列表
const currentSections = computed<ModuleSectionItem[]>(() => {
  if (!displayedModule.value) return []
  return MODULE_SECTIONS_CONFIG[displayedModule.value] ?? []
})
const activeSectionId = ref('')

watch(
  displayedModule,
  (mod) => {
    if (mod && MODULE_SECTIONS_CONFIG[mod]?.length) {
      if (props.initialSectionId && MODULE_SECTIONS_CONFIG[mod]!.some((item) => item.id === props.initialSectionId)) {
        activeSectionId.value = props.initialSectionId
      } else {
        activeSectionId.value = MODULE_SECTIONS_CONFIG[mod]![0]!.id
      }
    } else {
      activeSectionId.value = ''
    }
  },
  { immediate: true },
)

// 检测每个设置选项是否有未保存修改，供左侧导航展示未保存标记
const sectionDirty = computed<Record<string, boolean>>(() => {
  const current = draft.value
  const baseline = props.bundle
  const differs = (a: unknown, b: unknown) => JSON.stringify(a) !== JSON.stringify(b)

  return {
    // Editor
    'editor-format': current.database.defaultFormat !== baseline.database.defaultFormat,
    'editor-files': differs(current.files?.journal, baseline.files?.journal),
    'editor-skeleton': differs(current.editor, baseline.editor),
    'editor-prompt': current.ai.generationPreference !== baseline.ai.generationPreference,

    // Git
    'git-repositories': differs(current.git.repositories, baseline.git.repositories),
    'git-prompt': (current.git.commitPrompt ?? '') !== (baseline.git.commitPrompt ?? ''),
    'git-credentials': false,
    'git-advanced': current.git.executable !== baseline.git.executable,

    // Excel
    'excel-export': differs(current.excel, baseline.excel),
    'excel-files': differs(current.files?.excel, baseline.files?.excel),

    // Summary
    'summary-ai': false,
    'summary-prompt': current.ai.summaryPrompt !== baseline.ai.summaryPrompt,
    'summary-files': differs(current.files?.summary, baseline.files?.summary),

    // Calendar
    'calendar-sync': false,

    // History
    'history-limit': current.database.historyLimitBytes !== baseline.database.historyLimitBytes,
  }
})

function scrollToSection(targetId: string) {
  void nextTick(() => {
    if (panel.value) panel.value.scrollTop = 0
    const container = body.value
    if (!container) return
    const el = container.querySelector<HTMLElement>(
      `[heading-id="${targetId}"], [heading-id$="${targetId}"], #${targetId}, .${targetId}`,
    )
    if (el) {
      const section = el.closest<HTMLElement>('.module-settings-section') ?? el
      const containerRect = container.getBoundingClientRect()
      const targetRect = section.getBoundingClientRect()
      const top = Math.max(0, container.scrollTop + (targetRect.top - containerRect.top) - 12)
      container.scrollTo({ top, behavior: 'smooth' })
    }
  })
}

function selectSection(item: ModuleSectionItem) {
  activeSectionId.value = item.id
  scrollToSection(item.targetId)
}

function onBodyScroll() {
  const container = body.value
  if (!container) return
  const sections = currentSections.value
  const containerTop = container.getBoundingClientRect().top
  for (let i = sections.length - 1; i >= 0; i--) {
    const item = sections[i]!
    const el = container.querySelector(
      `[heading-id="${item.targetId}"], [heading-id$="${item.targetId}"], #${item.targetId}, .${item.targetId}`,
    )
    if (el) {
      const rect = el.getBoundingClientRect()
      if (rect.top - containerTop <= 80) {
        activeSectionId.value = item.id
        break
      }
    }
  }
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
      displayedModule.value = props.module
      if (!definition.value) {
        emit('close')
        return
      }
      resetDraft()
      const focused = document.activeElement
      if (focused instanceof HTMLElement && !panel.value?.contains(focused)) returnFocus = focused
      await nextTick()
      if (panel.value) panel.value.scrollTop = 0
      if (props.initialSectionId) {
        const target = currentSections.value.find((s) => s.id === props.initialSectionId)
        if (target) {
          selectSection(target)
        } else {
          body.value?.scrollTo({ top: 0 })
        }
      } else {
        body.value?.scrollTo({ top: 0 })
      }
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

onMounted(() => {
  if (panel.value) panel.value.scrollTop = 0
  window.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  motion.dispose()
})

function navigateToSection(sectionId: string) {
  const sections = currentSections.value
  const found = sections.find((item) => item.id === sectionId)
  if (found) {
    selectSection(found)
  }
}

defineExpose({ requestClose, unchanged, isOpen: () => props.visible, navigateToSection })
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
        <!-- 左侧模块自身选项导航侧栏 -->
        <aside class="module-settings-sidebar">
          <div class="module-settings-sidebar-header">
            <span class="module-settings-sidebar-icon">
              <component :is="moduleDefinition?.icon ?? PhGearSix" :size="18" aria-hidden="true" />
            </span>
            <div class="module-settings-sidebar-title-wrap">
              <span class="module-settings-sidebar-title">{{ definition?.title ?? '模块设置' }}</span>
              <span class="module-settings-sidebar-badge">设置选项</span>
            </div>
          </div>

          <nav class="module-settings-nav" :aria-label="`${definition?.title ?? '模块'}设置选项`">
            <button
              v-for="item in currentSections"
              :key="item.id"
              type="button"
              class="module-settings-nav-item"
              :class="{ 'is-active': activeSectionId === item.id }"
              :aria-current="activeSectionId === item.id ? 'page' : undefined"
              @click="selectSection(item)"
            >
              <span class="module-settings-nav-icon">
                <component :is="item.icon" :size="16" aria-hidden="true" />
              </span>
              <span class="module-settings-nav-copy">
                <strong>{{ item.title }}</strong>
                <small>{{ item.desc }}</small>
              </span>
              <span
                v-if="sectionDirty[item.id]"
                class="module-settings-dirty-dot"
                role="img"
                aria-label="有未保存修改"
                title="有未保存修改"
              ></span>
            </button>
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

          <div ref="body" class="module-settings-body" @scroll.passive="onBodyScroll">
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
