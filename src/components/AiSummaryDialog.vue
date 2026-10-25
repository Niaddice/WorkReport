<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { isTauri } from '@tauri-apps/api/core'
import {
  PhCircleNotch,
  PhFolderOpen,
  PhDownloadSimple,
  PhFileText,
  PhCopy,
  PhCheck,
  PhSparkle,
  PhMicrosoftExcelLogo,
} from '@phosphor-icons/vue'
import { api } from '../lib/workspace'
import type { FileFormat } from '../lib/workspace'
import { createDialogMotion } from '../lib/dialogMotion'
import type { SummaryMode } from '../lib/ai'
import type { ExcelExportResult, ExcelPreview } from '../lib/useExcel'
import ExcelPreviewDialog from './ExcelPreviewDialog.vue'

const props = defineProps<{
  /** 在模块内直接渲染，不使用原生 modal dialog。 */
  inline?: boolean
  visible: boolean
  /** 是否已经读取好所选日期的日报原文。未准备时在同一界面内显示“准备总结”。 */
  prepared?: boolean
  preparing?: boolean
  prepareError?: string
  /** 是否已配置默认 AI；未配置时 AI 总结仅提示前往设置。 */
  aiAvailable?: boolean
  generating: boolean
  elapsedSeconds: number
  dates: string[]
  result: string
  error: string
  provider: string
  mode: SummaryMode
  outputLoading: boolean
  outputExporting: boolean
  outputError: string
  exportedFile: string
  excelEnabled: boolean
  excelPreparing: boolean
  excelExporting: boolean
  excelBlockedReason: string
  excelResult: ExcelExportResult | null
  excelPreview: ExcelPreview | null
  excelError: string
}>()
const emit = defineEmits<{
  close: []
  ai: []
  direct: []
  export: [format: FileFormat]
  excel: []
  excelUnavailable: []
  cancelExcel: []
  confirmExcel: []
  discardExcel: []
  regenerateExcel: []
  openExcelDirectory: []
}>()
const dialog = ref<HTMLDialogElement>()
const dialogMotion = createDialogMotion(() => dialog.value)
const excelConfirmation = ref<HTMLDialogElement>()
const excelConfirmationMotion = createDialogMotion(() => excelConfirmation.value)
const excelConfirming = ref(false)
const copied = ref(false)
type MenuKind = 'export' | 'ai'
const activeMenu = ref<MenuKind | null>(null)
const exportMenuOpen = computed(() => activeMenu.value === 'export')
const aiMenuOpen = computed(() => activeMenu.value === 'ai')
const exportControl = ref<HTMLElement>()
const exportButton = ref<HTMLButtonElement>()
const exportMenu = ref<HTMLElement>()
const aiControl = ref<HTMLElement>()
const aiButton = ref<HTMLButtonElement>()
const aiMenu = ref<HTMLElement>()
const excelBusy = computed(() => props.excelPreparing || props.excelExporting)
const summaryActionDisabled = computed(
  () => !props.prepared || props.preparing || props.generating || props.outputExporting || excelBusy.value,
)
const exportDisabled = computed(
  () => props.generating || props.outputLoading || props.outputExporting || excelBusy.value || !props.result.trim(),
)
const rendered = computed(() =>
  DOMPurify.sanitize(marked.parse(props.result, { async: false }) as string, {
    FORBID_TAGS: ['img', 'style', 'input', 'a'],
    FORBID_ATTR: ['style'],
  }),
)
const rangeLabel = computed(() => {
  const dates = [...props.dates].sort()
  if (!dates.length) return '所选日期没有日报'
  const first = dayjs(dates[0])
  if (dates.length === 1) return first.format('YYYY 年 M 月 D 日')
  const last = dayjs(dates[dates.length - 1])
  return `${first.format('YYYY 年 M 月 D 日')} – ${last.format(last.isSame(first, 'year') ? 'M 月 D 日' : 'YYYY 年 M 月 D 日')}`
})
const countLabel = computed(() => `${props.dates.length} 天有日报`)
const progressText = computed(() => {
  if (props.elapsedSeconds < 3) return `正在整理 ${props.dates.length} 天的日报…`
  if (props.elapsedSeconds < 12) return 'AI 正在总结所选日报…'
  if (props.elapsedSeconds < 30) return '正在归纳重点，内容较多时会需要更久…'
  return '仍在等待 AI 返回，可停止后重试…'
})
const elapsedLabel = computed(() => {
  const seconds = props.elapsedSeconds
  return seconds < 60 ? `${seconds} 秒` : `${Math.floor(seconds / 60)} 分 ${String(seconds % 60).padStart(2, '0')} 秒`
})
watch(
  [() => props.visible, dialog],
  ([visible]) => {
    activeMenu.value = null
    if (!visible) cancelExcelConfirmation()
    if (props.inline) return
    if (visible) dialogMotion.open()
    else void dialogMotion.close()
  },
  { flush: 'post', immediate: true },
)
watch([() => props.excelEnabled, excelBusy, () => props.outputExporting], ([enabled, exporting, outputExporting]) => {
  if (!enabled || exporting || outputExporting) cancelExcelConfirmation()
})
watch(
  () => props.result,
  () => {
    copied.value = false
    activeMenu.value = null
  },
)
watch(exportDisabled, (disabled) => {
  if (disabled && exportMenuOpen.value) closeMenu()
})
function closeMenu(focus = false) {
  const button = activeMenu.value === 'ai' ? aiButton.value : exportButton.value
  activeMenu.value = null
  if (focus) button?.focus()
}
function menuItems(kind: MenuKind) {
  const menu = kind === 'ai' ? aiMenu.value : exportMenu.value
  return [...(menu?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])]
}
async function showMenu(kind: MenuKind, last = false) {
  if (kind === 'export' && exportDisabled.value) return
  activeMenu.value = kind
  await nextTick()
  if (activeMenu.value !== kind) return
  const items = menuItems(kind)
  items[last ? items.length - 1 : 0]?.focus()
}
function onMenuKeydown(kind: MenuKind, event: KeyboardEvent) {
  if (event.key === 'Escape' && activeMenu.value === kind) {
    event.preventDefault()
    event.stopPropagation()
    closeMenu(true)
    return
  }
  if (event.key === 'Tab') {
    closeMenu()
    return
  }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  if (activeMenu.value !== kind) {
    void showMenu(kind, event.key === 'ArrowUp' || event.key === 'End')
    return
  }
  const items = menuItems(kind)
  if (!items.length) return
  const index = items.indexOf(document.activeElement as HTMLButtonElement)
  const next =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? items.length - 1
        : (index + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length
  items[next]?.focus()
}
function exportAs(format: FileFormat) {
  closeMenu(true)
  emit('export', format)
}
function summarize(mode: 'ai' | 'direct') {
  if (!props.visible || summaryActionDisabled.value) return
  closeMenu(true)
  if (mode === 'ai') emit('ai')
  else emit('direct')
}
function onOutsidePointer(event: PointerEvent) {
  const control = activeMenu.value === 'ai' ? aiControl.value : exportControl.value
  if (!control?.contains(event.target as Node)) closeMenu()
}
onMounted(() => document.addEventListener('pointerdown', onOutsidePointer))
onBeforeUnmount(() => {
  excelConfirmationMotion.dispose()
  dialogMotion.dispose()
  document.removeEventListener('pointerdown', onOutsidePointer)
})
function requestExcel() {
  if (!props.visible || props.outputExporting || excelBusy.value) return
  if (props.excelBlockedReason) {
    emit('excelUnavailable')
    return
  }
  closeMenu()
  excelConfirming.value = true
  excelConfirmationMotion.open()
}
function cancelExcelConfirmation() {
  excelConfirming.value = false
  void excelConfirmationMotion.close()
}
async function confirmExcel() {
  if (!excelConfirming.value || !props.visible || props.outputExporting || excelBusy.value) return
  excelConfirming.value = false
  if (!(await excelConfirmationMotion.close()) || !props.visible || props.outputExporting || excelBusy.value) return
  emit('excel')
}
async function copySummary() {
  try {
    if (isTauri()) await api.copy(props.result)
    else await navigator.clipboard.writeText(props.result)
    copied.value = true
  } catch {
    /* The dialog stays usable if the clipboard is unavailable. */
  }
}
</script>

<template>
  <!-- eslint-disable vue/no-v-html -- AI 结果先经 DOMPurify 清洗再渲染 -->
  <Teleport to="body" :disabled="inline">
    <dialog
      ref="dialog"
      tabindex="-1"
      :open="inline ? true : undefined"
      :role="inline ? 'region' : 'dialog'"
      :aria-modal="inline ? undefined : 'true'"
      class="ai-dialog summary-dialog"
      :class="{ 'is-inline': inline }"
      aria-label="AI 总结"
      aria-describedby="summary-dialog-description"
      @cancel.prevent="!outputExporting && !excelExporting && emit('close')"
    >
      <header class="summary-header">
        <p id="summary-dialog-description" class="ai-dialog-description">
          {{ rangeLabel }}<span v-if="prepared && dates.length"> · {{ countLabel }}</span>
        </p>
        <div class="summary-result-actions" role="group" aria-label="总结操作">
          <button
            v-if="result"
            type="button"
            class="summary-result-icon summary-copy-icon"
            :title="copied ? '已复制' : '复制总结'"
            :aria-label="copied ? '已复制' : '复制总结'"
            @click="copySummary"
          >
            <Transition name="button-label" mode="out-in"
              ><PhCheck v-if="copied" key="copied" :size="14" /><PhCopy v-else key="copy" :size="14"
            /></Transition>
          </button>
          <div v-if="result" ref="exportControl" class="summary-export" @keydown="onMenuKeydown('export', $event)">
            <button
              ref="exportButton"
              type="button"
              class="summary-result-icon summary-export-icon"
              :disabled="exportDisabled"
              :title="outputExporting ? '正在导出' : '导出总结'"
              :aria-label="outputExporting ? '正在导出' : '导出总结'"
              aria-haspopup="menu"
              :aria-expanded="exportMenuOpen"
              aria-controls="summary-export-menu"
              @click="exportMenuOpen ? closeMenu() : showMenu('export')"
            >
              <PhCircleNotch v-if="outputExporting" class="spin" :size="14" aria-hidden="true" /><PhDownloadSimple
                v-else
                :size="14"
                aria-hidden="true"
              />
            </button>
            <Transition name="status-fade"
              ><div
                v-if="exportMenuOpen"
                id="summary-export-menu"
                ref="exportMenu"
                class="summary-export-menu"
                role="menu"
                aria-label="选择总结导出格式"
              >
                <button type="button" role="menuitem" @click="exportAs('md')">
                  <PhFileText :size="16" aria-hidden="true" /><span>Markdown <small>.md</small></span></button
                ><button type="button" role="menuitem" @click="exportAs('txt')">
                  <PhFileText :size="16" aria-hidden="true" /><span>TXT <small>.txt</small></span>
                </button>
              </div></Transition
            >
          </div>
          <div ref="aiControl" class="summary-export summary-ai-control" @keydown="onMenuKeydown('ai', $event)">
            <button
              ref="aiButton"
              type="button"
              class="summary-result-icon summary-ai-status"
              title="选择总结方式"
              aria-label="选择总结方式"
              aria-haspopup="menu"
              :aria-expanded="aiMenuOpen"
              aria-controls="summary-ai-menu"
              @click="aiMenuOpen ? closeMenu() : showMenu('ai')"
            >
              <PhSparkle :size="14" aria-hidden="true" />
            </button>
            <Transition name="status-fade">
              <div
                v-if="aiMenuOpen"
                id="summary-ai-menu"
                ref="aiMenu"
                class="summary-export-menu summary-ai-menu"
                role="menu"
                aria-label="选择总结方式"
              >
                <button
                  type="button"
                  role="menuitem"
                  class="summary-ai-action"
                  :disabled="summaryActionDisabled"
                  :aria-disabled="!aiAvailable || summaryActionDisabled"
                  :title="aiAvailable ? '使用默认 AI 生成总结' : '尚未设置默认 AI，点击查看设置提示'"
                  @click="summarize('ai')"
                >
                  <PhSparkle :size="16" aria-hidden="true" /><span
                    >AI 总结<small v-if="!aiAvailable">需设置</small></span
                  >
                </button>
                <button
                  type="button"
                  role="menuitem"
                  class="summary-direct-action"
                  :disabled="summaryActionDisabled"
                  @click="summarize('direct')"
                >
                  <PhFileText :size="16" aria-hidden="true" /><span>直接总结</span>
                </button>
              </div>
            </Transition>
          </div>
        </div>
      </header>
      <div class="summary-result" :aria-busy="generating">
        <Transition name="status-fade" mode="out-in">
          <div v-if="generating" key="generating" class="summary-empty">
            <section class="ai-progress summary-progress" aria-live="polite">
              <div class="ai-progress-head">
                <PhCircleNotch class="spin" :size="15" /><span>{{ progressText }}</span
                ><span class="ai-progress-time" aria-hidden="true">{{ elapsedLabel }}</span>
              </div>
              <div
                class="ai-progress-track"
                role="progressbar"
                aria-label="AI 总结进度"
                aria-valuetext="正在等待 AI 返回"
              >
                <span></span>
              </div>
              <p class="ai-progress-hint">
                AI 返回前可以继续等待，或<button type="button" class="text-action summary-stop" @click="emit('close')">
                  停止总结
                </button>
              </p>
            </section>
          </div>
          <pre v-else-if="result && mode === 'direct'" key="direct" class="summary-plain">{{ result }}</pre>
          <div v-else-if="result" key="result" class="summary-markdown" v-html="rendered"></div>
          <div v-else-if="error" key="error" class="summary-empty summary-error" role="alert">{{ error }}</div>
          <div v-else-if="!prepared" key="prepare" class="summary-empty" :class="{ 'summary-error': !!prepareError }">
            <template v-if="prepareError">{{ prepareError }}</template>
            <template v-else-if="preparing">正在读取所选日报原文…</template>
          </div>
          <div v-else key="empty" class="summary-empty">
            <span>点击右上角 AI 按钮<br />选择“AI 总结”或“直接总结”</span>
          </div>
        </Transition>
      </div>
      <Transition name="status-fade" mode="out-in">
        <p v-if="outputError" key="error" class="summary-export-feedback error-text" role="alert">{{ outputError }}</p>
        <p v-else-if="exportedFile" key="saved" class="summary-export-feedback settings-success" role="status">
          已导出：{{ exportedFile }}
        </p>
      </Transition>
      <section v-if="excelEnabled" id="summary-excel-status" class="summary-excel-feedback" aria-live="polite">
        <p v-if="excelPreparing">
          <PhCircleNotch class="spin" :size="14" aria-hidden="true" /><span>正在调用默认 AI，生成 Excel 填充预览…</span
          ><button type="button" class="text-action" @click="emit('cancelExcel')">停止生成</button>
        </p>
        <p v-else-if="excelExporting">
          <PhCircleNotch class="spin" :size="14" aria-hidden="true" /><span>正在保存已确认的 Excel…</span>
        </p>
        <p v-else-if="excelError" class="error-text" role="alert">{{ excelError }}</p>
        <p v-else-if="excelResult" class="settings-success">
          <span :title="excelResult.path">Excel 已输出：{{ excelResult.filename }}</span
          ><button type="button" class="text-action" @click="emit('openExcelDirectory')">
            <PhFolderOpen :size="14" aria-hidden="true" />打开输出目录
          </button>
        </p>
        <p v-else>{{ excelBlockedReason || '无需先生成总结；先调用 AI 生成填充预览，核对并确认后再输出 Excel。' }}</p>
      </section>
      <footer v-if="excelEnabled" class="ai-dialog-actions summary-dialog-actions">
        <div class="ai-dialog-confirm-actions">
          <button
            type="button"
            class="summary-export-toggle summary-excel-button"
            :disabled="outputExporting || excelBusy"
            :title="excelBlockedReason || '调用默认 AI 生成预览，确认内容后输出 Excel'"
            aria-describedby="summary-excel-status"
            @click="requestExcel"
          >
            <PhMicrosoftExcelLogo :size="15" aria-hidden="true" /><span>Excel 输出</span>
          </button>
        </div>
      </footer>
    </dialog>
    <dialog
      ref="excelConfirmation"
      tabindex="-1"
      role="alertdialog"
      aria-modal="true"
      class="confirmation-dialog excel-confirmation-dialog"
      aria-labelledby="excel-confirmation-title"
      aria-describedby="excel-confirmation-description"
      @cancel.prevent="cancelExcelConfirmation"
    >
      <div class="dialog-icon"><PhMicrosoftExcelLogo :size="28" weight="light" /></div>
      <h2 id="excel-confirmation-title">调用 AI 生成 Excel 预览？</h2>
      <p id="excel-confirmation-description">
        将调用默认 AI「{{ provider || '未设置' }}」，读取所选 {{ dates.length }} 天的日报与 Excel 模板，按照 Excel
        输出提示词生成填充预览。核对预览并确认后，才会保存 Excel 文件。
      </p>
      <p class="excel-confirmation-range">{{ rangeLabel }}</p>
      <div class="dialog-actions">
        <md-text-button class="dialog-button dialog-button-secondary" @click="cancelExcelConfirmation"
          >取消</md-text-button
        ><md-filled-button class="dialog-button dialog-button-primary" @click="confirmExcel">生成预览</md-filled-button>
      </div>
    </dialog>
  </Teleport>
  <ExcelPreviewDialog
    :visible="visible && !!excelPreview"
    :preview="excelPreview"
    :exporting="excelExporting"
    :error="excelError"
    @close="emit('discardExcel')"
    @confirm="emit('confirmExcel')"
    @regenerate="emit('regenerateExcel')"
  />
</template>
