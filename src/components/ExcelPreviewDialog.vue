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
  contentPrompt?: string
  content?: string
  reviewingContent?: boolean
  generatingContent?: boolean
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
  fill: []
  'edit-content': []
  configure: []
  'update:prompt': [value: string]
  'update:contentPrompt': [value: string]
  'update:content': [value: string]
}>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const displayed = shallowRef<ExcelPreview | null>(null)
const view = ref<'sheet' | 'changes'>('sheet')
const session = computed(() => props.prompt !== undefined)
const twoStep = computed(() => props.contentPrompt !== undefined)
const step = computed(() =>
  twoStep.value
    ? props.configuring || props.generatingContent
      ? 0
      : 1
    : props.configuring
      ? 0
      : props.preparing
        ? 1
        : 2,
)
const steps = computed(() => (twoStep.value ? ['生成内容', '填充 Excel'] : ['确认生成', 'AI 处理', '预览导出']))
const activePrompt = computed(() => (twoStep.value && props.configuring ? props.contentPrompt : props.prompt))
const promptError = computed(() =>
  (activePrompt.value?.length ?? 0) > 6000
    ? '提示词不能超过 6000 字。'
    : (!twoStep.value || !props.configuring) && !activePrompt.value?.trim()
      ? '请填写 Excel 填充提示词。'
      : '',
)
const contentError = computed(() =>
  !props.content?.trim()
    ? '请保留需要填入 Excel 的报告内容。'
    : new TextEncoder().encode(props.content).length > 500_000
      ? '报告内容不能超过 500 KB，请精简后再填充。'
      : '',
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
watch(
  [() => props.preparing, () => props.configuring, () => props.reviewingContent],
  ([preparing, configuring, reviewing]) => {
    if (preparing || configuring || reviewing) displayed.value = null
  },
)
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
        <span>{{
          twoStep
            ? configuring
              ? '生成内容'
              : generatingContent
                ? '正在生成内容'
                : reviewingContent
                  ? '确认内容与填充规则'
                  : preparing
                    ? '正在填充 Excel'
                    : '预览与导出'
            : configuring
              ? '确认生成'
              : preparing
                ? 'AI 处理中'
                : '预览与导出'
        }}</span>
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
          generatingContent
            ? '正在整理所选日报。生成后先由你核对内容，再填入 Excel。'
            : preparing
              ? '正在将已确认的内容填入 Excel 模板，完成后显示工作表预览。'
              : configuring
                ? twoStep
                  ? '第一步：核对日期和内容生成提示词，先生成可编辑的报告正文。'
                  : '核对日期与提示词，确认后使用默认 AI 生成报告。'
                : reviewingContent
                  ? '第二步：核对或修改报告正文，确认后按填充提示词写入 Excel 模板。'
                  : '请核对工作表中的内容与格式，点击“确认输出”后保存 Excel 文件。'
        }}
      </p>
      <ol v-if="session" class="excel-workflow-steps" aria-label="Excel报告生成流程">
        <li
          v-for="(label, index) in steps"
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
            <label :for="twoStep ? 'excel-session-content-prompt' : 'excel-session-prompt'">{{
              twoStep ? '内容生成提示词' : '本次生成提示词'
            }}</label
            ><span>仅本次有效</span>
          </div>
          <textarea
            :id="twoStep ? 'excel-session-content-prompt' : 'excel-session-prompt'"
            :value="activePrompt"
            maxlength="6000"
            spellcheck="false"
            aria-describedby="excel-session-prompt-hint"
            :aria-invalid="!!promptError || undefined"
            :placeholder="
              twoStep
                ? '说明如何归纳日报、组织报告正文；留空使用默认规则…'
                : '输入本次报告的整理方式、填写位置和格式要求…'
            "
            @input="
              twoStep
                ? emit('update:contentPrompt', ($event.target as HTMLTextAreaElement).value)
                : emit('update:prompt', ($event.target as HTMLTextAreaElement).value)
            "
          />
          <div class="excel-generation-editor-hint">
            <p id="excel-session-prompt-hint">
              {{
                twoStep
                  ? '只决定报告写什么，不包含单元格和格式规则；留空使用默认内容规则。修改仅本次有效。'
                  : '修改仅用于本次生成，不会同步到模块设置；关闭后恢复 Excel 设置中的提示词。'
              }}
            </p>
            <span>{{ activePrompt?.length ?? 0 }} / 6000</span>
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
              >{{ twoStep ? '生成报告内容' : '确认并生成' }}</md-filled-button
            >
          </div>
        </footer>
      </template>
      <template v-else-if="reviewingContent && !preparing">
        <div class="excel-content-review">
          <section class="excel-generation-editor excel-confirmed-content-field">
            <div class="excel-generation-editor-heading">
              <label for="excel-confirmed-content">待填充内容</label><span>核对后可直接修改</span>
            </div>
            <textarea
              id="excel-confirmed-content"
              :value="content"
              spellcheck="false"
              aria-describedby="excel-confirmed-content-hint"
              :aria-invalid="!!contentError || undefined"
              @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
            />
            <div class="excel-generation-editor-hint">
              <p id="excel-confirmed-content-hint">
                填表时仅使用这里确认的正文。修改内容后，生成的 Excel 预览会以本次确认内容为准。
              </p>
              <span>{{ content?.length ?? 0 }} 字</span>
            </div>
          </section>
          <section class="excel-generation-editor">
            <div class="excel-generation-editor-heading">
              <label for="excel-session-prompt">Excel 填充提示词</label><span>仅本次有效</span>
            </div>
            <textarea
              id="excel-session-prompt"
              :value="prompt"
              maxlength="6000"
              spellcheck="false"
              aria-describedby="excel-fill-prompt-hint"
              :aria-invalid="!!promptError || undefined"
              placeholder="说明填写的工作表、单元格、固定字段及样式…"
              @input="emit('update:prompt', ($event.target as HTMLTextAreaElement).value)"
            />
            <div class="excel-generation-editor-hint">
              <p id="excel-fill-prompt-hint">只决定已确认内容怎么填入模板，不重新生成报告正文。</p>
              <span>{{ prompt?.length ?? 0 }} / 6000</span>
            </div>
          </section>
          <div class="excel-fill-template">
            <PhMicrosoftExcelLogo :size="16" aria-hidden="true" /><span>填充模板</span
            ><strong :title="templateName">{{ templateName || '尚未选择模板' }}</strong>
          </div>
          <p v-if="blockedReason" class="settings-error" role="alert">
            {{ blockedReason
            }}<button type="button" class="text-action" @click="emit('configure')">前往 Excel 设置</button>
          </p>
          <p v-else-if="contentError || promptError || error" class="settings-error" role="alert">
            {{ contentError || promptError || error }}
          </p>
        </div>
        <footer class="excel-generation-actions">
          <button type="button" class="text-action excel-edit-content" @click="emit('edit-content')">
            <PhArrowClockwise :size="14" aria-hidden="true" />返回内容生成
          </button>
          <div class="ai-dialog-confirm-actions">
            <md-text-button class="dialog-button dialog-button-secondary" data-window-close @click="emit('close')"
              >取消</md-text-button
            >
            <md-filled-button
              class="dialog-button dialog-button-primary excel-confirm-content"
              :disabled="!!blockedReason || !!contentError || !!promptError || preparing || exporting"
              @click="emit('fill')"
              >确认内容并填充 Excel</md-filled-button
            >
          </div>
        </footer>
      </template>
      <details v-else-if="session" class="excel-session-prompt">
        <summary>本次提示词</summary>
        <template v-if="twoStep"
          ><strong>内容生成提示词</strong>
          <pre>{{ contentPrompt || '使用默认内容生成规则' }}</pre>
          <strong>Excel 填充提示词</strong></template
        >
        <pre>{{ prompt }}</pre>
      </details>
      <template v-if="!configuring && preparing">
        <div class="excel-generation-progress" :aria-busy="true">
          <p v-if="scopeLabel" class="excel-generation-scope">{{ scopeLabel }}</p>
          <div class="ai-progress" role="status" aria-live="polite">
            <div class="ai-progress-head">
              <PhCircleNotch class="spin" :size="18" aria-hidden="true" />
              <strong>{{ generatingContent ? 'AI 正在生成报告内容…' : 'AI 正在填充 Excel 模板…' }}</strong>
              <span class="ai-progress-time">已用时 {{ elapsedSeconds ?? 0 }} 秒</span>
            </div>
            <div class="ai-progress-track" role="progressbar" aria-label="Excel报告生成进度"><span /></div>
            <p class="ai-progress-hint">
              {{
                generatingContent
                  ? '完成后将显示报告正文供你核对，不会自动填充 Excel。'
                  : '仅将已确认的内容映射到模板，完成后可核对工作表预览。'
              }}
            </p>
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
        <div class="excel-preview-tabs island-tabs" role="group" aria-label="Excel 预览方式">
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
              <PhArrowClockwise :size="14" aria-hidden="true" />{{
                twoStep ? '返回修改内容与填充规则' : session ? '修改提示词并重新生成' : '重新生成'
              }}
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
      <div v-else-if="!configuring && !reviewingContent && error" class="excel-generation-progress">
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

<style src="./excelWorkflow.css"></style>
