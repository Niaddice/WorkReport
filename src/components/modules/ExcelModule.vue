<script setup lang="ts">
import { computed, ref } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhCircleNotch, PhFolderOpen, PhMicrosoftExcelLogo } from '@phosphor-icons/vue'
import ExcelPreviewDialog from '../ExcelPreviewDialog.vue'
import { sameWeekRange } from '../../lib/useExcel'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal, ai, excel, actions, summarySelectionDates, excelBlockedReason, disabled } = context
const { selectedDate } = journal
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
} = excel

const actionError = ref('')
const directoryOpening = ref(false)

/** 有框选范围时输出整个范围，否则输出选中日期所在自然周（周一起）7 天。 */
const dates = computed(() => {
  if (context.rangeDates.value.length) return [...context.rangeDates.value].sort()
  const base = dayjs(selectedDate.value)
  const start = base.subtract((base.day() + 6) % 7, 'day')
  return Array.from({ length: 7 }, (_, index) => start.add(index, 'day').format('YYYYMMDD'))
})
const providerName = computed(() => defaultProvider.value?.name ?? '')
const scopeLabel = computed(() => rangeLabel(dates.value))
const statusLabel = computed(() => {
  if (settingsLoading.value) return '正在读取设置…'
  return settings.value.enabled ? '已启用' : '未启用'
})
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

function rangeLabel(values: string[]) {
  if (!values.length) return '未选择日期'
  const first = dayjs(values[0])
  if (values.length === 1) return first.format('YYYY 年 M 月 D 日')
  const last = dayjs(values[values.length - 1])
  return `${first.format('YYYY 年 M 月 D 日')} – ${last.format(last.isSame(first, 'year') ? 'M 月 D 日' : 'YYYY 年 M 月 D 日')}`
}

async function prepare() {
  if (preparing.value || exporting.value) return
  actionError.value = ''
  try {
    await actions.prepareExcel([...dates.value])
  } catch (cause) {
    actionError.value = `生成 Excel 预览失败：${String(cause)}`
  }
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
    <div v-else-if="result" class="excel-module-result">
      <p class="excel-module-result-title">
        <PhMicrosoftExcelLogo :size="16" aria-hidden="true" /><span
          >Excel 已输出：<strong :title="result.path">{{ result.filename }}</strong></span
        >
      </p>
      <p class="excel-module-result-path" :title="result.path">{{ result.path }}</p>
      <p class="excel-module-result-meta">已填充 {{ result.cells }} 个单元格</p>
      <p v-if="exportError || actionError" class="error-text" role="alert">{{ exportError || actionError }}</p>
      <div class="excel-module-actions">
        <md-text-button
          type="button"
          class="dialog-button dialog-button-secondary"
          :disabled="directoryOpening"
          aria-label="打开 Excel 输出目录"
          @click="openDirectory"
        >
          <PhCircleNotch v-if="directoryOpening" slot="icon" class="spin" :size="14" aria-hidden="true" /><PhFolderOpen
            v-else
            slot="icon"
            :size="15"
            aria-hidden="true"
          />打开输出目录
        </md-text-button>
        <md-text-button
          type="button"
          class="dialog-button dialog-button-secondary"
          :disabled="preparing || exporting || disabled"
          aria-label="重新生成 Excel 预览"
          @click="prepare"
        >
          <PhArrowClockwise slot="icon" :size="15" aria-hidden="true" />重新生成
        </md-text-button>
      </div>
    </div>
    <div v-else class="excel-module-setup">
      <p>Excel 周报会读取同一周的日报，调用默认 AI 生成填充预览；核对预览并确认后才写入文件。</p>
      <p class="excel-module-scope">范围：{{ scopeLabel }}（{{ dates.length }} 天）</p>
      <div class="excel-module-status" role="status" aria-live="polite">
        <p>Excel 输出：{{ statusLabel }}</p>
        <p>默认 AI：{{ providerName || '未设置' }}</p>
        <p v-if="preparing">
          <PhCircleNotch class="spin" :size="14" aria-hidden="true" />正在调用默认 AI，生成 Excel 填充预览…
        </p>
        <p v-else-if="exportError" class="error-text" role="alert">{{ exportError }}</p>
        <p v-else-if="settingsError" class="error-text" role="alert">{{ settingsError }}</p>
        <p v-else-if="actionError" class="error-text" role="alert">{{ actionError }}</p>
        <p v-else-if="blockedReason">{{ blockedReason }}</p>
        <p v-else>已就绪，可以生成 Excel 预览。</p>
      </div>
      <div class="excel-module-actions">
        <md-filled-button
          type="button"
          class="dialog-button dialog-button-primary"
          :disabled="preparing || exporting || settingsLoading || disabled"
          aria-label="生成 Excel 预览"
          @click="prepare"
        >
          <PhCircleNotch v-if="preparing" slot="icon" class="spin" :size="14" aria-hidden="true" /><PhMicrosoftExcelLogo
            v-else
            slot="icon"
            :size="15"
            aria-hidden="true"
          />{{ preparing ? '正在生成预览…' : '生成 Excel 预览' }}
        </md-filled-button>
      </div>
    </div>
  </div>
</template>
