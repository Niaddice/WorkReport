<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, useId, watch } from 'vue'
import { PhGearSix, PhX } from '@phosphor-icons/vue'
import { moduleById } from '../modules/registry'
import { MODULE_SETTINGS_CONTEXT, type ModuleSettingsContext } from '../modules/settingsContext'
import type { ModuleType } from '../modules/types'
import type { Workspace } from '../lib/workspace'
import { useSettingsDraft, type WorkspaceSettings } from '../lib/settingsDraft'
import { aiSettingsReady } from '../lib/ai'
import type { HolidayState } from '../lib/holidays'
import type { GlobalSettingsSection } from '../lib/settingsSections'

const props = defineProps<{
  visible: boolean
  /** 当前打开的模块；没有模块时弹窗为空。 */
  module: ModuleType | null
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

const moduleDefinition = computed(() => (props.module ? moduleById(props.module) : undefined))
const definition = computed(() => moduleDefinition.value?.settings)
const savable = computed(() => definition.value?.savable !== false)
const titleId = useId()
const panel = ref<HTMLElement>()
const body = ref<HTMLElement>()
const content = ref<{ validate?: () => boolean; focusInvalid?: () => void }>()
const localError = ref('')
type PendingAction = { kind: 'close' } | { kind: 'global'; section: GlobalSettingsSection }
const pendingAction = ref<PendingAction | null>(null)
let returnFocus: HTMLElement | null = null

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

const busy = computed(() => props.busy || saving.value)
const statusError = computed(() => props.error || localError.value || draftError.value)
const aiReady = computed(() => aiSettingsReady(draft.value.ai))

function requestGlobalSettings(section: GlobalSettingsSection) {
  if (busy.value) return
  if (!savable.value || unchanged.value) {
    emit('open-global-settings', section)
    return
  }
  pendingAction.value = { kind: 'global', section }
}
function requestClose() {
  if (busy.value) return
  if (!savable.value || unchanged.value) {
    emit('close')
    return
  }
  pendingAction.value = { kind: 'close' }
}
function cancelPending() {
  pendingAction.value = null
}
function confirmDiscard() {
  const action = pendingAction.value
  if (!action) return
  pendingAction.value = null
  localError.value = ''
  resetDraft()
  if (action.kind === 'global') emit('open-global-settings', action.section)
  else emit('close')
}
function onBackdrop() {
  if (pendingAction.value) cancelPending()
  else requestClose()
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
  if (await saveDraft()) emit('close')
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
  () => props.visible,
  async (visible) => {
    if (visible) {
      pendingAction.value = null
      localError.value = ''
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
    } else {
      pendingAction.value = null
      localError.value = ''
      await nextTick()
      if (!props.visible && returnFocus?.isConnected) returnFocus.focus({ preventScroll: true })
      returnFocus = null
    }
  },
  { immediate: true, flush: 'post' },
)
watch(
  () => props.module,
  () => {
    if (props.visible) resetDraft()
  },
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
})

defineExpose({ requestClose, unchanged, isOpen: () => props.visible })
</script>

<template>
  <Transition name="module-settings-fade">
    <div v-show="visible" class="module-settings-layer" :inert="!visible">
      <div class="module-settings-backdrop" aria-hidden="true" @click.self="onBackdrop"></div>
      <section
        ref="panel"
        class="module-settings-dialog"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        :aria-describedby="definition?.description ? `${titleId}-description` : undefined"
        tabindex="-1"
      >
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
            type="button"
            class="module-settings-close"
            aria-label="关闭模块设置"
            title="关闭（Esc）"
            :disabled="busy"
            @click="requestClose"
          >
            <PhX :size="17" aria-hidden="true" />
          </button>
        </header>
        <div ref="body" class="module-settings-body">
          <component :is="definition?.component" v-if="definition && visible" ref="content" />
        </div>
        <footer class="module-settings-footer">
          <div
            class="module-settings-status"
            :class="{ 'has-changes': savable && !unchanged, 'has-error': !!statusError }"
            aria-live="polite"
            aria-atomic="true"
          >
            <p v-if="statusError" class="settings-error" role="alert">{{ statusError }}</p>
            <p v-else-if="saving">正在保存设置…</p>
            <p v-else-if="busy">正在读取设置…</p>
            <p v-else-if="!savable">同步后立即生效，无需保存。</p>
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
            <button v-if="savable" type="button" class="text-action" @click="requestClose">
              {{ unchanged ? '关闭' : '取消' }}
            </button>
            <button
              v-if="savable"
              type="button"
              class="settings-save-button module-settings-save"
              :disabled="busy || unchanged"
              @click="saveSettings"
            >
              {{ saving ? '正在保存…' : '保存设置' }}
            </button>
            <button v-else type="button" class="settings-save-button module-settings-save" @click="requestClose">
              完成
            </button>
          </div>
        </footer>
      </section>
    </div>
  </Transition>
</template>
