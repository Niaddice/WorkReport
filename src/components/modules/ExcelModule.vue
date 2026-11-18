<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhCircleNotch, PhFileXls, PhFolderOpen, PhMicrosoftExcelLogo } from '@phosphor-icons/vue'
import ExcelPreviewDialog from '../ExcelPreviewDialog.vue'
import ExcelExportPreviewDialog from '../ExcelExportPreviewDialog.vue'
import {
  RECOMMENDED_EXCEL_CONTENT_PROMPT,
  reportOverlapsRange,
  validExcelDates,
  type ExcelExportFile,
} from '../../lib/useExcel'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal, ai, excel, actions, summarySelectionDates, disabled } = context
const { selectedDate, workspace } = journal
const { defaultProvider } = ai
const {
  settings,
  loading: settingsLoading,
  error: settingsError,
  preparing,
  generatingContent,
  contentDraft,
  exporting,
  preview,
  result,
  exportError,
  exports,
  exportsLoading,
  exportsError,
  exportPreview,
  previewOpening,
  previewOpenError,
  loadExports,
  openExport,
  closeExportPreview,
} = excel

const actionError = ref('')
const directoryOpening = ref(false)
const previewFilename = ref('')
const generationOpen = ref(preparing.value || generatingContent.value || !!contentDraft.value)
const generationDates = ref<string[]>([...summarySelectionDates.value])
const generationStarting = ref(false)
const generationPhase = ref<'content' | 'fill'>('content')
const generationEditing = ref(!preparing.value && !generatingContent.value && !preview.value && !contentDraft.value)
const generationPrompt = ref(excel.previewPrompt.value || settings.value.prompt)
const generationContentPrompt = ref(
  generatingContent.value || contentDraft.value
    ? excel.contentPrompt.value
    : (settings.value.contentPrompt ?? RECOMMENDED_EXCEL_CONTENT_PROMPT),
)
const generationContent = ref(contentDraft.value)
const generationTemplate = computed(() => settings.value.templatePath.split(/[\\/]/).pop() ?? '')
const generationDirectory = computed(() => {
  const directory = context.fileSettings?.value.excel.directory ?? excel.fileRule.value.directory
  return `${(workspace.value?.path ?? '').replace(/[\\/]+$/, '')}/${directory}`
})
const elapsedSeconds = ref(0)
let generationRevision = 0
let progressTimer: ReturnType<typeof setInterval> | undefined
watch(
  contentDraft,
  (value, previous) => {
    if (value || !previous) return
    generationRevision++
    generationStarting.value = false
    generationContent.value = ''
    generationEditing.value = true
    generationOpen.value = false
  },
  { flush: 'sync' },
)
const generationBusy = computed(() => generationStarting.value || preparing.value || generatingContent.value)
const contentBusy = computed(
  () => generatingContent.value || (generationStarting.value && generationPhase.value === 'content'),
)
const reviewingContent = computed(
  () => !generationEditing.value && !generationBusy.value && !preview.value && !!contentDraft.value,
)
watch(
  generationBusy,
  (busy) => {
    clearInterval(progressTimer)
    if (!busy) return
    elapsedSeconds.value = 0
    const started = Date.now()
    progressTimer = setInterval(() => {
      elapsedSeconds.value = Math.floor((Date.now() - started) / 1000)
    }, 1000)
  },
  { immediate: true },
)

/** 生成与筛选均使用日历的实际选择，单日不再扩展为整周。 */
const dates = computed(() =>
  context.rangeDates.value.length ? [...new Set(context.rangeDates.value)].sort() : [selectedDate.value],
)
const scopeLabel = computed(() => rangeLabel(dates.value))
const visibleExports = computed(() => {
  const current = dates.value
  return exports.value.filter((file) => reportOverlapsRange(file, current[0]!, current[current.length - 1]!))
})
const localBlockedReason = computed(() => {
  if (!validExcelDates(dates.value)) return '请选择有效的日期或日期范围。'
  if (!defaultProvider.value) return '请先在设置中配置并选择默认 AI。'
  if (!settings.value.enabled) return '请先在 Excel 设置中启用输出。'
  if (!settings.value.templatePath) return '请先在 Excel 设置中选择模板。'
  return ''
})
const blockedReason = localBlockedReason

function rangeLabel(values: string[]) {
  if (!values.length) return '未选择日期'
  const first = dayjs(values[0])
  if (values.length === 1) return first.format('YYYY 年 M 月 D 日')
  const last = dayjs(values[values.length - 1])
  return `${first.format('YYYY 年 M 月 D 日')} – ${last.format(last.isSame(first, 'year') ? 'M 月 D 日' : 'YYYY 年 M 月 D 日')}`
}

function refreshExports() {
  void loadExports(workspace.value?.path ?? '')
}

onBeforeUnmount(() => clearInterval(progressTimer))
watch(
  () => workspace.value?.path,
  () => {
    generationRevision++
    generationStarting.value = false
    generationOpen.value = false
  },
)
watch(
  [
    () => workspace.value?.path ?? '',
    () => settings.value.enabled,
    generationDirectory,
    () => excel.fileRule.value.filenameTemplate,
    dates,
  ],
  () => refreshExports(),
  {
    immediate: true,
  },
)
watch(result, (value) => {
  if (value) {
    generationOpen.value = false
    refreshExports()
  }
})

function requestGeneration() {
  if (generationBusy.value || exporting.value || disabled.value) return
  actions.discardExcelPreview()
  actionError.value = ''
  exportError.value = ''
  generationDates.value = [...dates.value]
  generationPrompt.value = settings.value.prompt
  generationContentPrompt.value = settings.value.contentPrompt ?? RECOMMENDED_EXCEL_CONTENT_PROMPT
  generationContent.value = ''
  generationPhase.value = 'content'
  generationEditing.value = true
  generationOpen.value = true
}
function editGeneration() {
  if (generationBusy.value || exporting.value) return
  excel.discardWorkbookPreview()
  actionError.value = ''
  exportError.value = ''
  generationEditing.value = false
}
function editContent() {
  if (generationBusy.value || exporting.value) return
  excel.discardWorkbookPreview()
  actionError.value = ''
  exportError.value = ''
  generationEditing.value = true
}
function configureExcel() {
  closeGeneration()
  actions.openExcelSettings()
}
async function generateContent() {
  if (
    generationBusy.value ||
    exporting.value ||
    disabled.value ||
    blockedReason.value ||
    generationContentPrompt.value.length > 6000
  )
    return
  const revision = ++generationRevision
  actionError.value = ''
  generationOpen.value = true
  generationStarting.value = true
  generationPhase.value = 'content'
  generationEditing.value = false
  try {
    await actions.generateExcelContent([...generationDates.value], generationContentPrompt.value)
    if (revision === generationRevision && !exportError.value && contentDraft.value.trim())
      generationContent.value = contentDraft.value
  } catch (cause) {
    if (revision === generationRevision) actionError.value = `生成报告内容失败：${String(cause)}`
  } finally {
    if (revision === generationRevision) {
      generationStarting.value = false
      if (!contentDraft.value.trim()) generationEditing.value = true
    }
  }
}
async function prepare() {
  if (
    generationBusy.value ||
    exporting.value ||
    disabled.value ||
    blockedReason.value ||
    !generationContent.value.trim() ||
    !generationPrompt.value.trim() ||
    generationPrompt.value.length > 6000
  )
    return
  const revision = ++generationRevision
  actionError.value = ''
  generationOpen.value = true
  generationStarting.value = true
  generationPhase.value = 'fill'
  generationEditing.value = false
  try {
    await actions.prepareExcel([...generationDates.value], generationPrompt.value, generationContent.value)
  } catch (cause) {
    if (revision === generationRevision) actionError.value = `生成 Excel 预览失败：${String(cause)}`
  } finally {
    if (revision === generationRevision) {
      generationStarting.value = false
    }
  }
}
function closeGeneration() {
  if (exporting.value) return
  generationRevision++
  generationStarting.value = false
  generationOpen.value = false
  actions.discardExcelPreview()
}

async function openDirectory() {
  if (directoryOpening.value) return
  directoryOpening.value = true
  actionError.value = ''
  try {
    await actions.openExcelOutputDirectory()
  } catch (cause) {
    actionError.value = `打开输出目录失败：${String(cause)}`
  } finally {
    directoryOpening.value = false
  }
}

function previewExport(file: ExcelExportFile) {
  previewFilename.value = file.filename
  void openExport(workspace.value?.path ?? '', file.path)
}

function formatModified(value: number) {
  return value ? dayjs(value).format('YYYY-MM-DD HH:mm') : ''
}
function formatSize(value: number) {
  if (value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`
  if (value >= 1024) return `${Math.round(value / 1024)} KB`
  return `${value} B`
}
</script>

<template>
  <div class="excel-module" :aria-busy="generationBusy || exporting || settingsLoading">
    <div class="excel-module-main">
      <p class="excel-module-range">
        <strong>{{ scopeLabel }}</strong>
        <span class="excel-module-range-days">{{ dates.length }} 天</span>
      </p>
      <p v-if="blockedReason" class="excel-module-hint">{{ blockedReason }}</p>
      <p v-else-if="exportError || actionError || settingsError" class="excel-module-hint error-text" role="alert">
        {{ exportError || actionError || settingsError }}
      </p>
      <hr class="excel-module-divider" />
      <section class="excel-exports" aria-label="已导出的报告">
        <div class="excel-exports-header">
          <h3>已导出</h3>
          <div class="excel-module-actions" role="group" aria-label="Excel报告操作">
            <button
              type="button"
              class="excel-generate-button"
              :disabled="generationBusy || exporting || settingsLoading || disabled"
              :title="blockedReason || '生成 Excel报告'"
              aria-label="生成 Excel报告"
              aria-haspopup="dialog"
              :aria-expanded="generationOpen || !!preview"
              aria-controls="excel-generation-dialog"
              @click="requestGeneration"
            >
              <PhCircleNotch v-if="generationBusy" class="spin" :size="16" aria-hidden="true" />
              <PhFileXls v-else :size="16" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="excel-text-button"
              :disabled="directoryOpening"
              aria-label="打开输出目录"
              title="打开输出目录"
              @click="openDirectory"
            >
              <PhCircleNotch v-if="directoryOpening" class="spin" :size="16" aria-hidden="true" />
              <PhFolderOpen v-else :size="16" aria-hidden="true" />
            </button>
            <button
              type="button"
              class="excel-text-button"
              :disabled="exportsLoading"
              aria-label="刷新已导出报告"
              title="刷新已导出报告"
              @click="refreshExports"
            >
              <PhArrowClockwise :size="16" aria-hidden="true" />
            </button>
          </div>
        </div>
        <div class="excel-exports-content">
          <p v-if="exportsLoading" class="excel-exports-state">正在读取已导出报告…</p>
          <p v-else-if="exportsError" class="excel-exports-state error-text" role="alert">{{ exportsError }}</p>
          <p v-else-if="!exports.length" class="excel-exports-state">暂无已导出的报告</p>
          <p v-else-if="!visibleExports.length" class="excel-exports-state">当前选择日期没有对应的报告</p>
          <ul v-else class="excel-export-list">
            <li v-for="file in visibleExports" :key="file.path">
              <button type="button" class="excel-export-item" :title="file.path" @click="previewExport(file)">
                <PhMicrosoftExcelLogo :size="16" aria-hidden="true" />
                <span class="excel-export-name">{{ file.filename }}</span>
                <span class="excel-export-meta">{{ formatModified(file.modified) }} · {{ formatSize(file.size) }}</span>
              </button>
            </li>
          </ul>
        </div>
      </section>
    </div>
    <ExcelPreviewDialog
      v-model:prompt="generationPrompt"
      v-model:content-prompt="generationContentPrompt"
      v-model:content="generationContent"
      :configuring="generationEditing"
      :reviewing-content="reviewingContent"
      :generating-content="contentBusy"
      :provider="defaultProvider?.name ?? ''"
      :template-name="generationTemplate"
      :output-directory="generationDirectory"
      :blocked-reason="blockedReason"
      :visible="generationOpen || !!preview"
      :preview="preview"
      :preparing="generationBusy"
      :elapsed-seconds="elapsedSeconds"
      :scope-label="rangeLabel(generationDates)"
      :exporting="exporting"
      :error="exportError || actionError"
      @close="closeGeneration"
      @confirm="actions.confirmExcel"
      @generate="generateContent"
      @fill="prepare"
      @edit-content="editContent"
      @regenerate="editGeneration"
      @configure="configureExcel"
    />
    <ExcelExportPreviewDialog
      :visible="!!exportPreview || previewOpening || !!previewOpenError"
      :filename="exportPreview?.filename ?? previewFilename"
      :sheets="exportPreview?.sheets ?? []"
      :loading="previewOpening"
      :error="previewOpenError"
      @close="closeExportPreview"
    />
  </div>
</template>
