<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { isTauri } from '@tauri-apps/api/core'
import {
  PhSparkle,
  PhCircleNotch,
  PhFolderOpen,
  PhDownloadSimple,
  PhCaretDown,
  PhFileText,
  PhMicrosoftExcelLogo,
} from '@phosphor-icons/vue'
import { api } from '../lib/workspace'
import type { FileFormat } from '../lib/workspace'
import { createDialogMotion } from '../lib/dialogMotion'
import type { SummaryMode } from '../lib/ai'
import type { ExcelExportResult, ExcelPreview } from '../lib/useExcel'
import ExcelPreviewDialog from './ExcelPreviewDialog.vue'

const props = defineProps<{
  visible: boolean
  generating: boolean
  elapsedSeconds: number
  dates: string[]
  result: string
  error: string
  provider: string
  mode: SummaryMode
  outputDirectory: string
  outputLoading: boolean
  outputExporting: boolean
  directoryOpening: boolean
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
  openDirectory: []
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
const exportMenuOpen = ref(false)
const exportControl = ref<HTMLElement>()
const exportButton = ref<HTMLButtonElement>()
const exportMenu = ref<HTMLElement>()
const excelBusy = computed(() => props.excelPreparing || props.excelExporting)
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
const modeHint = computed(() => {
  if (props.generating) return `正在由 ${props.provider || '默认 AI'} 生成总结…`
  if (props.mode === 'direct') return '已按日期合并为纯文本，未调用 AI。'
  if (props.mode === 'ai' && props.result) return `由 ${props.provider || '默认 AI'} 生成。`
  return 'AI 总结会调用默认 AI；直接总结只合并所选日报原文。'
})
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
    exportMenuOpen.value = false
    if (!visible) cancelExcelConfirmation()
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
    exportMenuOpen.value = false
  },
)
watch(exportDisabled, (disabled) => {
  if (disabled) exportMenuOpen.value = false
})
function closeExportMenu(focus = false) {
  exportMenuOpen.value = false
  if (focus) exportButton.value?.focus()
}
async function showExportMenu(last = false) {
  if (exportDisabled.value) return
  exportMenuOpen.value = true
  await nextTick()
  const items = exportMenu.value?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')
  items?.[last ? items.length - 1 : 0]?.focus()
}
function onExportKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && exportMenuOpen.value) {
    event.preventDefault()
    event.stopPropagation()
    closeExportMenu(true)
    return
  }
  if (event.key === 'Tab') {
    closeExportMenu()
    return
  }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  if (!exportMenuOpen.value) {
    void showExportMenu(event.key === 'ArrowUp' || event.key === 'End')
    return
  }
  const items = [...(exportMenu.value?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])]
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
  closeExportMenu(true)
  emit('export', format)
}
function onOutsidePointer(event: PointerEvent) {
  if (!exportControl.value?.contains(event.target as Node)) closeExportMenu()
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
  closeExportMenu()
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
  <Teleport to="body">
    <dialog
      ref="dialog"
      tabindex="-1"
      role="dialog"
      aria-modal="true"
      class="ai-dialog summary-dialog"
      aria-labelledby="summary-dialog-title"
      aria-describedby="summary-dialog-description"
      @cancel.prevent="!outputExporting && !excelExporting && emit('close')"
    >
      <header class="ai-dialog-heading">
        <div>
          <PhSparkle :size="23" />
          <h2 id="summary-dialog-title">总结</h2>
        </div>
        <span>{{ provider || '未设置默认 AI' }}</span>
      </header>
      <p id="summary-dialog-description" class="ai-dialog-description">
        {{ rangeLabel }}<span v-if="dates.length"> · {{ countLabel }}</span>
      </p>
      <div class="summary-modes" role="group" aria-label="总结方式">
        <md-filled-button
          class="dialog-button dialog-button-primary"
          :disabled="generating || outputExporting || excelBusy"
          @click="emit('ai')"
          >AI 总结</md-filled-button
        >
        <md-text-button
          class="dialog-button dialog-button-secondary"
          :disabled="generating || outputExporting || excelBusy"
          @click="emit('direct')"
          >直接总结</md-text-button
        >
        <p class="summary-mode-hint" role="status">{{ modeHint }}</p>
      </div>
      <Transition name="status-fade"
        ><section v-if="generating" class="ai-progress" aria-live="polite">
          <div class="ai-progress-head">
            <PhCircleNotch class="spin" :size="15" /><span>{{ progressText }}</span
            ><span class="ai-progress-time" aria-hidden="true">{{ elapsedLabel }}</span>
          </div>
          <div class="ai-progress-track" role="progressbar" aria-label="AI 总结进度" aria-valuetext="正在等待 AI 返回">
            <span></span>
          </div>
          <p class="ai-progress-hint">AI 返回前可以继续等待，或点击“停止总结”。</p>
        </section></Transition
      >
      <div class="summary-result" :aria-busy="generating">
        <Transition name="status-fade" mode="out-in">
          <div v-if="generating" key="generating" class="summary-empty">
            <span><PhCircleNotch class="spin" :size="18" /> 正在生成总结…</span>
          </div>
          <pre v-else-if="result && mode === 'direct'" key="direct" class="summary-plain">{{ result }}</pre>
          <div v-else-if="result" key="result" class="summary-markdown" v-html="rendered"></div>
          <div v-else-if="error" key="error" class="summary-empty summary-error" role="alert">{{ error }}</div>
          <div v-else key="empty" class="summary-empty">选择“AI 总结”或“直接总结”<br />结果会显示在这里</div>
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
      <footer class="ai-dialog-actions summary-dialog-actions">
        <div class="summary-directory">
          <button
            type="button"
            class="summary-directory-button"
            :disabled="!outputDirectory || outputLoading || directoryOpening || outputExporting"
            :title="outputDirectory"
            aria-label="打开总结输出目录"
            @click="emit('openDirectory')"
          >
            <PhCircleNotch v-if="directoryOpening" class="spin" :size="15" aria-hidden="true" /><PhFolderOpen
              v-else
              :size="15"
              aria-hidden="true"
            /><span>{{ outputLoading ? '正在读取总结目录…' : outputDirectory || '尚未选择工作空间' }}</span>
          </button>
        </div>
        <div class="ai-dialog-confirm-actions">
          <button class="text-action summary-copy" :disabled="generating || !result" @click="copySummary">
            <Transition name="button-label" mode="out-in"
              ><span :key="String(copied)">{{ copied ? '已复制' : '复制' }}</span></Transition
            >
          </button>
          <div ref="exportControl" class="summary-export" @keydown="onExportKeydown">
            <button
              ref="exportButton"
              type="button"
              class="summary-export-toggle"
              :disabled="exportDisabled"
              aria-haspopup="menu"
              :aria-expanded="exportMenuOpen"
              aria-controls="summary-export-menu"
              @click="exportMenuOpen ? closeExportMenu() : showExportMenu()"
            >
              <PhCircleNotch v-if="outputExporting" class="spin" :size="14" aria-hidden="true" /><PhDownloadSimple
                v-else
                :size="14"
                aria-hidden="true"
              /><span>{{ outputExporting ? '正在导出' : '导出' }}</span
              ><PhCaretDown :size="12" aria-hidden="true" />
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
          <button
            v-if="excelEnabled"
            type="button"
            class="summary-export-toggle summary-excel-button"
            :disabled="outputExporting || excelBusy"
            :title="excelBlockedReason || '调用默认 AI 生成预览，确认内容后输出 Excel'"
            aria-describedby="summary-excel-status"
            @click="requestExcel"
          >
            <PhMicrosoftExcelLogo :size="15" aria-hidden="true" /><span>Excel 输出</span>
          </button>
          <md-text-button
            class="dialog-button dialog-button-secondary"
            :disabled="outputExporting || excelExporting"
            @click="emit('close')"
            ><Transition name="button-label" mode="out-in"
              ><span :key="generating ? 'ai' : excelPreparing ? 'excel' : 'idle'">{{
                generating ? '停止总结' : excelPreparing ? '停止并关闭' : '关闭'
              }}</span></Transition
            ></md-text-button
          >
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
