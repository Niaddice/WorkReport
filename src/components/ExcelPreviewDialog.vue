<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhCircleNotch, PhMicrosoftExcelLogo, PhFolderOpen, PhX } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import type { ExcelPreview } from '../lib/useExcel'
import ExcelSheetPreview from './ExcelSheetPreview.vue'

type ExcelPreviewStyle = ExcelPreview['styles'][number]

const props = defineProps<{
  inline?: boolean
  visible: boolean
  preview: ExcelPreview | null
  preparing?: boolean
  elapsedSeconds?: number
  scopeLabel?: string
  configuring?: boolean
  prompt?: string
  provider?: string
  templateName?: string
  outputDirectory?: string
  blockedReason?: string
  exporting: boolean
  error: string
}>()
const emit = defineEmits<{
  close: []
  confirm: []
  regenerate: []
  generate: []
  configure: []
  'update:prompt': [value: string]
}>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const displayed = shallowRef<ExcelPreview | null>(null)
const view = ref<'sheet' | 'changes'>('sheet')
const session = computed(() => props.prompt !== undefined)
const step = computed(() => (props.configuring ? 0 : props.preparing ? 1 : 2))
const promptError = computed(() =>
  !props.prompt?.trim() ? '请填写本次生成使用的提示词。' : props.prompt.length > 6000 ? '提示词不能超过 6000 字。' : '',
)
watch(
  () => props.preview,
  (value) => {
    if (!value) return
    if (value.id !== displayed.value?.id) view.value = value.sheets?.length ? 'sheet' : 'changes'
    displayed.value = value
  },
  { immediate: true },
)
watch([() => props.preparing, () => props.configuring], ([preparing, configuring]) => {
  if (preparing || configuring) displayed.value = null
})
watch(
  [() => props.visible, dialog],
  ([visible]) => {
    if (props.inline) return
    if (visible) motion.open()
    else void motion.close()
  },
  { immediate: true, flush: 'post' },
)
onBeforeUnmount(() => motion.dispose())
const sheets = computed(() => {
  const preview = displayed.value
  if (!preview) return []
  const names = [
    ...new Set([...preview.cells.map((cell) => cell.sheet), ...preview.styles.map((style) => style.sheet)]),
  ]
  return names.map((name) => ({
    name,
    cells: preview.cells.filter((cell) => cell.sheet === name),
    styles: preview.styles.filter((style) => style.sheet === name),
  }))
})
const rangeLabel = computed(() => {
  if (!displayed.value) return ''
  const { start, end } = displayed.value
  const first = dayjs(start)
  const last = dayjs(end)
  return start === end
    ? first.format('YYYY 年 M 月 D 日')
    : `${first.format('YYYY 年 M 月 D 日')} – ${last.format(last.isSame(first, 'year') ? 'M 月 D 日' : 'YYYY 年 M 月 D 日')}`
})
function valueText(value: string | number | boolean | null): string {
  return value === null || value === ''
    ? '（清空内容）'
    : typeof value === 'boolean'
      ? value
        ? 'TRUE'
        : 'FALSE'
      : String(value)
}
function styleText(style: ExcelPreviewStyle): string {
  return [
    style.border === 'black' ? '黑色细边框' : '',
    style.wrapText === true ? '自动换行' : style.wrapText === false ? '取消自动换行' : '',
    style.autoRowHeight === true ? '按内容调整行高' : style.autoRowHeight === false ? '保持原行高' : '',
  ]
    .filter(Boolean)
    .join(' · ')
}
</script>

<template>
  <Teleport to="body" :disabled="inline">
    <dialog
      :id="session ? 'excel-generation-dialog' : undefined"
      ref="dialog"
      tabindex="-1"
      :open="inline ? true : undefined"
      :role="inline ? 'region' : 'dialog'"
      :aria-modal="inline ? undefined : 'true'"
      class="ai-dialog excel-preview-dialog"
      :class="{ 'is-inline': inline }"
      aria-labelledby="excel-preview-title"
      aria-describedby="excel-preview-description"
      @cancel.prevent="!exporting && emit('close')"
    >
      <header class="ai-dialog-heading">
        <div>
          <PhMicrosoftExcelLogo :size="23" />
          <h2 id="excel-preview-title">{{ session ? 'Excel报告' : 'Excel 填充预览' }}</h2>
        </div>
        <span>{{ configuring ? '确认生成' : preparing ? 'AI 处理中' : '预览与导出' }}</span>
        <button
          v-if="!inline"
          type="button"
          class="toolbar-icon-button"
          data-window-close="default"
          aria-label="关闭 Excel报告"
          :disabled="exporting"
          @click="emit('close')"
        >
          <PhX :size="16" />
        </button>
      </header>
      <p id="excel-preview-description" class="ai-dialog-description">
        {{
          preparing
            ? '正在根据所选日报填充 Excel 模板，完成后自动显示预览。'
            : configuring
              ? '核对日期与提示词，确认后使用默认 AI 生成报告。'
              : '请核对工作表中的内容与格式，点击“确认输出”后保存 Excel 文件。'
        }}
      </p>
      <ol v-if="session" class="excel-workflow-steps" aria-label="Excel报告生成流程">
        <li
          v-for="(label, index) in ['确认生成', 'AI 处理', '预览导出']"
          :key="label"
          :class="{ 'is-current': step === index, 'is-complete': step > index }"
          :aria-current="step === index ? 'step' : undefined"
        >
          <span>{{ index + 1 }}</span
          >{{ label }}
        </li>
      </ol>
      <template v-if="configuring">
        <div class="excel-session-overview">
          <div>
            <span>报告日期</span><strong>{{ scopeLabel }}</strong>
          </div>
          <div>
            <span>使用 AI</span><strong>{{ provider || '尚未配置' }}</strong>
          </div>
          <div>
            <span>Excel 模板</span><strong :title="templateName">{{ templateName || '尚未选择' }}</strong>
          </div>
        </div>
        <div class="excel-generation-editor">
          <div class="excel-generation-editor-heading">
            <label for="excel-session-prompt">本次生成提示词</label><span>仅本次有效</span>
          </div>
          <textarea
            id="excel-session-prompt"
            :value="prompt"
            maxlength="6000"
            spellcheck="false"
            aria-describedby="excel-session-prompt-hint"
            :aria-invalid="!prompt?.trim() || undefined"
            placeholder="输入本次报告的整理方式、填写位置和格式要求…"
            @input="emit('update:prompt', ($event.target as HTMLTextAreaElement).value)"
          />
          <div class="excel-generation-editor-hint">
            <p id="excel-session-prompt-hint">修改仅用于本次生成，不会同步到设置；关闭后恢复设置中的提示词。</p>
            <span>{{ prompt?.length ?? 0 }} / 6000</span>
          </div>
          <p v-if="blockedReason" class="settings-error" role="alert">
            {{ blockedReason }}
            <button type="button" class="text-action" @click="emit('configure')">前往 Excel 设置</button>
          </p>
          <p v-else-if="promptError || error" class="settings-error" role="alert">{{ promptError || error }}</p>
        </div>
        <footer class="excel-generation-actions">
          <div class="excel-preview-directory" :title="outputDirectory">
            <PhFolderOpen :size="15" aria-hidden="true" /><span>{{ outputDirectory }}</span>
          </div>
          <div class="ai-dialog-confirm-actions">
            <md-text-button class="dialog-button dialog-button-secondary" data-window-close @click="emit('close')"
              >取消</md-text-button
            >
            <md-filled-button
              class="dialog-button dialog-button-primary excel-start-generation"
              :disabled="!!blockedReason || !!promptError || preparing || exporting"
              @click="emit('generate')"
              >确认并生成</md-filled-button
            >
          </div>
        </footer>
      </template>
      <details v-else-if="session" class="excel-session-prompt">
        <summary>本次提示词</summary>
        <pre>{{ prompt }}</pre>
      </details>
      <template v-if="!configuring && preparing">
        <div class="excel-generation-progress" :aria-busy="true">
          <p v-if="scopeLabel" class="excel-generation-scope">{{ scopeLabel }}</p>
          <div class="ai-progress" role="status" aria-live="polite">
            <div class="ai-progress-head">
              <PhCircleNotch class="spin" :size="18" aria-hidden="true" />
              <strong>AI 正在处理所选日报…</strong>
              <span class="ai-progress-time">已用时 {{ elapsedSeconds ?? 0 }} 秒</span>
            </div>
            <div class="ai-progress-track" role="progressbar" aria-label="Excel报告生成进度"><span /></div>
            <p class="ai-progress-hint">正在生成填充内容并准备工作表预览，内容较多时需要更久。</p>
          </div>
        </div>
        <footer class="excel-generation-actions">
          <div v-if="outputDirectory" class="excel-preview-directory" :title="outputDirectory">
            <PhFolderOpen :size="15" aria-hidden="true" /><span>{{ outputDirectory }}</span>
          </div>
          <div class="ai-dialog-confirm-actions">
            <md-text-button class="dialog-button dialog-button-secondary" data-window-close @click="emit('close')"
              >取消生成</md-text-button
            >
          </div>
        </footer>
      </template>
      <template v-else-if="!configuring && displayed">
        <div class="excel-preview-meta">
          <strong :title="displayed.templateName">{{ displayed.templateName }}</strong
          ><span>{{ rangeLabel }}</span
          ><span>{{ displayed.cells.length }} 个填充单元格 · {{ displayed.styles.length }} 处格式设置</span>
        </div>
        <div class="excel-preview-tabs" role="group" aria-label="Excel 预览方式">
          <button
            type="button"
            :aria-pressed="view === 'sheet'"
            :disabled="!displayed.sheets?.length"
            @click="view = 'sheet'"
          >
            工作表</button
          ><button type="button" :aria-pressed="view === 'changes'" @click="view = 'changes'">填充明细</button>
        </div>
        <div class="excel-preview-stage" :aria-busy="exporting">
          <Transition name="excel-preview-view"
            ><div v-show="view === 'sheet'" class="excel-preview-view excel-preview-workbook" :inert="view !== 'sheet'">
              <ExcelSheetPreview :sheets="displayed.sheets ?? []" :preview-id="displayed.id" /></div
          ></Transition>
          <Transition name="excel-preview-view"
            ><div
              v-show="view === 'changes'"
              class="excel-preview-view excel-preview-content"
              :inert="view !== 'changes'"
              tabindex="0"
              aria-label="Excel 填充内容预览"
            >
              <section
                v-for="sheet in sheets"
                :key="sheet.name"
                class="excel-preview-sheet"
                :aria-label="`工作表 ${sheet.name}`"
              >
                <h3>{{ sheet.name }}</h3>
                <table v-if="sheet.cells.length" class="excel-preview-table">
                  <caption class="sr-only">
                    {{
                      sheet.name
                    }}
                    的填充内容
                  </caption>
                  <thead>
                    <tr>
                      <th scope="col">单元格</th>
                      <th scope="col">填充内容</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="cell in sheet.cells" :key="cell.cell">
                      <th scope="row">
                        <span>{{ cell.cell }}</span
                        ><small v-if="cell.replacesFormula" class="excel-preview-formula">替换原公式</small>
                      </th>
                      <td>
                        <div
                          class="excel-preview-value"
                          :class="{ 'is-empty': cell.value === null || cell.value === '' }"
                        >
                          {{ valueText(cell.value) }}
                        </div>
                        <details v-if="cell.previousValue" class="excel-preview-previous">
                          <summary>原内容</summary>
                          <p>{{ cell.previousValue }}</p>
                        </details>
                      </td>
                    </tr>
                  </tbody>
                </table>
                <ul v-if="sheet.styles.length" class="excel-preview-styles" aria-label="格式设置">
                  <li v-for="(style, index) in sheet.styles" :key="index">
                    <strong>{{ style.range }}</strong
                    ><span>{{ styleText(style) }}</span>
                  </li>
                </ul>
              </section>
            </div>
          </Transition>
        </div>
        <Transition name="status-fade"
          ><p v-if="error" class="settings-error excel-preview-error" role="alert">{{ error }}</p></Transition
        >
        <footer class="ai-dialog-actions excel-preview-actions">
          <div class="excel-preview-directory" :title="displayed.outputDirectory">
            <PhFolderOpen :size="15" aria-hidden="true" /><span>{{ displayed.outputDirectory }}</span>
          </div>
          <div class="ai-dialog-confirm-actions">
            <button type="button" class="text-action" :disabled="exporting" @click="emit('regenerate')">
              <PhArrowClockwise :size="14" aria-hidden="true" />{{ session ? '修改提示词并重新生成' : '重新生成' }}
            </button>
            <md-text-button
              class="dialog-button dialog-button-secondary"
              data-window-close
              :disabled="exporting"
              @click="emit('close')"
              >返回</md-text-button
            >
            <md-filled-button
              class="dialog-button dialog-button-primary"
              data-window-close
              :disabled="exporting || !preview"
              @click="emit('confirm')"
              ><PhCircleNotch v-if="exporting" slot="icon" class="spin" :size="14" aria-hidden="true" />{{
                exporting ? '正在输出…' : '确认输出'
              }}</md-filled-button
            >
          </div>
        </footer>
      </template>
      <div v-else-if="!configuring && error" class="excel-generation-progress">
        <p class="settings-error" role="alert">{{ error }}</p>
        <div class="ai-dialog-confirm-actions">
          <md-text-button class="dialog-button dialog-button-secondary" data-window-close @click="emit('close')"
            >返回</md-text-button
          >
          <md-filled-button class="dialog-button dialog-button-primary" @click="emit('regenerate')"
            >重新生成</md-filled-button
          >
        </div>
      </div>
    </dialog>
  </Teleport>
</template>
