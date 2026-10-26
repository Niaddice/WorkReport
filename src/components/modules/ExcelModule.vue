<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhCircleNotch, PhFileXls, PhFolderOpen, PhMicrosoftExcelLogo } from '@phosphor-icons/vue'
import ExcelPreviewDialog from '../ExcelPreviewDialog.vue'
import ExcelExportPreviewDialog from '../ExcelExportPreviewDialog.vue'
import { sameWeekRange, type ExcelExportFile } from '../../lib/useExcel'
import { pushMessage } from '../../lib/messages'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal, ai, excel, actions, summarySelectionDates, excelBlockedReason, disabled } = context
const { selectedDate, workspace } = journal
const { defaultProvider } = ai
const {
  settings,
  loading: settingsLoading,
  error: settingsError,
  preparing,
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

/** 有框选范围时输出整个范围，否则输出选中日期所在自然周（周一起）7 天。 */
const dates = computed(() => {
  if (context.rangeDates.value.length) return [...context.rangeDates.value].sort()
  const base = dayjs(selectedDate.value)
  const start = base.subtract((base.day() + 6) % 7, 'day')
  return Array.from({ length: 7 }, (_, index) => start.add(index, 'day').format('YYYYMMDD'))
})
const scopeLabel = computed(() => rangeLabel(dates.value))
const localBlockedReason = computed(() => {
  if (!sameWeekRange(dates.value)) return '仅支持同一周内的日期范围（周一至周日），包括没有日报的所选日期。'
  if (!defaultProvider.value) return '请先在设置中配置并选择默认 AI。'
  if (!settings.value.enabled) return '请先在设置中启用 Excel 输出。'
  if (!settings.value.templatePath || !settings.value.prompt.trim() || !settings.value.outputDirectory)
    return '请先完成 Excel 输出设置。'
  return ''
})
/** 页面级校验优先，但只有在它针对当前日期范围时才采用，避免显示上一次准备留下的旧结论。 */
const blockedReason = computed(() => {
  const selection = summarySelectionDates.value
  const current = dates.value
  const sameSelection = selection.length === current.length && selection.every((date, index) => date === current[index])
  return (sameSelection ? excelBlockedReason.value : '') || localBlockedReason.value
})
/** 是否只是配置缺失（可引导去设置），而不是日期范围不合规。 */
const setupMissing = computed(
  () =>
    !defaultProvider.value ||
    !settings.value.enabled ||
    !settings.value.templatePath ||
    !settings.value.prompt.trim() ||
    !settings.value.outputDirectory,
)

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

onMounted(refreshExports)
watch(
  () => [workspace.value?.path ?? '', settings.value.outputDirectory, settings.value.enabled] as const,
  () => refreshExports(),
)
watch(result, (value) => {
  if (value) refreshExports()
})

function prepare() {
  if (blockedReason.value) {
    pushMessage({
      tone: 'info',
      text: blockedReason.value,
      duration: 6000,
      actions: setupMissing.value ? [{ label: '前往 Excel 设置', run: () => actions.openExcelSettings() }] : [],
    })
    return
  }
  if (preparing.value || exporting.value) return
  actionError.value = ''
  void Promise.resolve(actions.prepareExcel([...dates.value])).catch((cause: unknown) => {
    actionError.value = `生成 Excel 预览失败：${String(cause)}`
  })
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
  <div class="excel-module" :aria-busy="preparing || exporting || settingsLoading">
    <div v-if="preview" class="excel-module-panel">
      <ExcelPreviewDialog
        inline
        :visible="true"
        :preview="preview"
        :exporting="exporting"
        :error="exportError"
        @close="actions.discardExcelPreview"
        @confirm="actions.confirmExcel"
        @regenerate="prepare"
      />
    </div>
    <div v-else class="excel-module-main">
      <p class="excel-module-range">
        <span>日期范围</span>
        <strong>{{ scopeLabel }}</strong>
        <span class="excel-module-range-days">{{ dates.length }} 天</span>
      </p>
      <div class="excel-module-actions" role="group" aria-label="Excel 周报操作">
        <button
          type="button"
          class="excel-generate-button"
          :disabled="preparing || exporting || settingsLoading || disabled"
          :title="blockedReason || '调用默认 AI 生成填充预览'"
          @click="prepare"
        >
          <PhCircleNotch v-if="preparing" class="spin" :size="15" aria-hidden="true" />
          <PhFileXls v-else :size="15" aria-hidden="true" />
          <span>Xls</span>
        </button>
        <button type="button" class="excel-text-button" :disabled="directoryOpening" @click="openDirectory">
          <PhCircleNotch v-if="directoryOpening" class="spin" :size="14" aria-hidden="true" />
          <PhFolderOpen v-else :size="14" aria-hidden="true" />
          <span>打开目录</span>
        </button>
        <button type="button" class="excel-text-button" :disabled="exportsLoading" @click="refreshExports">
          <PhArrowClockwise :size="14" aria-hidden="true" />
          <span>刷新</span>
        </button>
      </div>
      <p v-if="blockedReason" class="excel-module-hint">{{ blockedReason }}</p>
      <p v-else-if="preparing" class="excel-module-hint">
        <PhCircleNotch class="spin" :size="14" aria-hidden="true" />正在调用默认 AI，生成 Excel 填充预览…
      </p>
      <p v-else-if="exportError || actionError || settingsError" class="excel-module-hint error-text" role="alert">
        {{ exportError || actionError || settingsError }}
      </p>
      <hr class="excel-module-divider" />
      <section class="excel-exports" aria-label="已导出的周报">
        <p v-if="exportsLoading" class="excel-exports-state">正在读取已导出周报…</p>
        <p v-else-if="exportsError" class="excel-exports-state error-text" role="alert">{{ exportsError }}</p>
        <p v-else-if="!exports.length" class="excel-exports-state">暂无已导出的周报</p>
        <ul v-else class="excel-export-list">
          <li v-for="file in exports" :key="file.path">
            <button type="button" class="excel-export-item" :title="file.path" @click="previewExport(file)">
              <PhMicrosoftExcelLogo :size="16" aria-hidden="true" />
              <span class="excel-export-name">{{ file.filename }}</span>
              <span class="excel-export-meta">{{ formatModified(file.modified) }} · {{ formatSize(file.size) }}</span>
            </button>
          </li>
        </ul>
      </section>
    </div>
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
