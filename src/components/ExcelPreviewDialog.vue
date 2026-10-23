<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhCircleNotch, PhMicrosoftExcelLogo, PhFolderOpen } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import type { ExcelPreview } from '../lib/useExcel'
import ExcelSheetPreview from './ExcelSheetPreview.vue'

type ExcelPreviewStyle = ExcelPreview['styles'][number]

const props = defineProps<{ visible: boolean; preview: ExcelPreview | null; exporting: boolean; error: string }>()
const emit = defineEmits<{ close: []; confirm: []; regenerate: [] }>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const displayed = shallowRef<ExcelPreview | null>(null)
const view = ref<'sheet' | 'changes'>('sheet')
watch(() => props.preview, value => {
  if (!value) return
  if (value.id !== displayed.value?.id) view.value = value.sheets?.length ? 'sheet' : 'changes'
  displayed.value = value
}, { immediate: true })
watch([() => props.visible, dialog], ([visible]) => { visible ? motion.open() : void motion.close() }, { immediate: true, flush: 'post' })
onBeforeUnmount(() => motion.dispose())
const sheets = computed(() => {
  const preview = displayed.value
  if (!preview) return []
  const names = [...new Set([...preview.cells.map(cell => cell.sheet), ...preview.styles.map(style => style.sheet)])]
  return names.map(name => ({ name, cells: preview.cells.filter(cell => cell.sheet === name), styles: preview.styles.filter(style => style.sheet === name) }))
})
const rangeLabel = computed(() => {
  if (!displayed.value) return ''
  const { start, end } = displayed.value
  const first = dayjs(start); const last = dayjs(end)
  return start === end ? first.format('YYYY 年 M 月 D 日') : `${first.format('YYYY 年 M 月 D 日')} – ${last.format(last.isSame(first, 'year') ? 'M 月 D 日' : 'YYYY 年 M 月 D 日')}`
})
function valueText(value: string | number | boolean | null): string {
  return value === null || value === '' ? '（清空内容）' : typeof value === 'boolean' ? value ? 'TRUE' : 'FALSE' : String(value)
}
function styleText(style: ExcelPreviewStyle): string {
  return [style.border === 'black' ? '黑色细边框' : '', style.wrapText === true ? '自动换行' : style.wrapText === false ? '取消自动换行' : '', style.autoRowHeight === true ? '按内容调整行高' : style.autoRowHeight === false ? '保持原行高' : ''].filter(Boolean).join(' · ')
}
</script>

<template>
  <Teleport to="body">
    <dialog ref="dialog" tabindex="-1" role="dialog" aria-modal="true" class="ai-dialog excel-preview-dialog" aria-labelledby="excel-preview-title" aria-describedby="excel-preview-description" @cancel.prevent="!exporting && emit('close')">
      <header class="ai-dialog-heading"><div><PhMicrosoftExcelLogo :size="23" /><h2 id="excel-preview-title">Excel 填充预览</h2></div><span>第 2 步 · 确认输出</span></header>
      <p id="excel-preview-description" class="ai-dialog-description">请核对工作表中的内容与格式，点击“确认输出”后保存 Excel 文件。</p>
      <template v-if="displayed">
        <div class="excel-preview-meta"><strong :title="displayed.templateName">{{ displayed.templateName }}</strong><span>{{ rangeLabel }}</span><span>{{ displayed.cells.length }} 个填充单元格 · {{ displayed.styles.length }} 处格式设置</span></div>
        <div class="excel-preview-tabs" role="group" aria-label="Excel 预览方式"><button type="button" :aria-pressed="view === 'sheet'" :disabled="!displayed.sheets?.length" @click="view = 'sheet'">工作表</button><button type="button" :aria-pressed="view === 'changes'" @click="view = 'changes'">填充明细</button></div>
        <div class="excel-preview-stage" :aria-busy="exporting">
        <Transition name="excel-preview-view"><div v-show="view === 'sheet'" class="excel-preview-view excel-preview-workbook" :inert="view !== 'sheet'"><ExcelSheetPreview :sheets="displayed.sheets ?? []" :preview-id="displayed.id" /></div></Transition>
        <Transition name="excel-preview-view"><div v-show="view === 'changes'" class="excel-preview-view excel-preview-content" :inert="view !== 'changes'" tabindex="0" aria-label="Excel 填充内容预览">
          <section v-for="sheet in sheets" :key="sheet.name" class="excel-preview-sheet" :aria-label="`工作表 ${sheet.name}`">
            <h3>{{ sheet.name }}</h3>
            <table v-if="sheet.cells.length" class="excel-preview-table"><caption class="sr-only">{{ sheet.name }} 的填充内容</caption><thead><tr><th scope="col">单元格</th><th scope="col">填充内容</th></tr></thead><tbody>
              <tr v-for="cell in sheet.cells" :key="cell.cell"><th scope="row"><span>{{ cell.cell }}</span><small v-if="cell.replacesFormula" class="excel-preview-formula">替换原公式</small></th><td><div class="excel-preview-value" :class="{ 'is-empty': cell.value === null || cell.value === '' }">{{ valueText(cell.value) }}</div><details v-if="cell.previousValue" class="excel-preview-previous"><summary>原内容</summary><p>{{ cell.previousValue }}</p></details></td></tr>
            </tbody></table>
            <ul v-if="sheet.styles.length" class="excel-preview-styles" aria-label="格式设置"><li v-for="(style, index) in sheet.styles" :key="index"><strong>{{ style.range }}</strong><span>{{ styleText(style) }}</span></li></ul>
          </section>
        </div>
        </Transition>
        </div>
        <Transition name="status-fade"><p v-if="error" class="settings-error excel-preview-error" role="alert">{{ error }}</p></Transition>
        <footer class="ai-dialog-actions excel-preview-actions">
          <div class="excel-preview-directory" :title="displayed.outputDirectory"><PhFolderOpen :size="15" aria-hidden="true" /><span>{{ displayed.outputDirectory }}</span></div>
          <div class="ai-dialog-confirm-actions">
            <button type="button" class="text-action" :disabled="exporting" @click="emit('regenerate')"><PhArrowClockwise :size="14" aria-hidden="true" />重新生成</button>
            <md-text-button class="dialog-button dialog-button-secondary" :disabled="exporting" @click="emit('close')">返回</md-text-button>
            <md-filled-button class="dialog-button dialog-button-primary" :disabled="exporting || !preview" @click="emit('confirm')"><PhCircleNotch v-if="exporting" slot="icon" class="spin" :size="14" aria-hidden="true" />{{ exporting ? '正在输出…' : '确认输出' }}</md-filled-button>
          </div>
        </footer>
      </template>
    </dialog>
  </Teleport>
</template>
