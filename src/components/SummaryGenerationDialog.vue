<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { isTauri } from '@tauri-apps/api/core'
import { PhArrowClockwise, PhCheck, PhCircleNotch, PhCopy, PhFolderOpen, PhSparkle, PhX } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import { api, type FileFormat } from '../lib/workspace'

const props = defineProps<{
  visible: boolean
  configuring: boolean
  prompt: string
  provider: string
  dates: string[]
  reportCount: number
  generating: boolean
  elapsedSeconds: number
  result: string
  error: string
  blockedReason: string
  exporting: boolean
  outputDirectory: string
  outputError: string
}>()
const emit = defineEmits<{
  close: []
  generate: []
  edit: []
  configure: []
  export: [format: FileFormat]
  'update:prompt': [value: string]
}>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const copied = ref(false)
const step = computed(() => (props.configuring ? 0 : props.generating ? 1 : 2))
const promptError = computed(() =>
  !props.prompt.trim() ? '请填写本次总结提示词。' : props.prompt.length > 2000 ? '提示词不能超过 2000 字。' : '',
)
const rangeLabel = computed(() => {
  const dates = [...props.dates].sort()
  if (!dates.length) return '未选择日期'
  const start = dayjs(dates[0])
  const end = dayjs(dates.at(-1))
  return dates.length === 1
    ? start.format('YYYY 年 M 月 D 日')
    : `${start.format('YYYY 年 M 月 D 日')} – ${end.format(end.isSame(start, 'year') ? 'M 月 D 日' : 'YYYY 年 M 月 D 日')}`
})
const rendered = computed(() =>
  DOMPurify.sanitize(marked.parse(props.result, { async: false }) as string, {
    FORBID_TAGS: ['img', 'style', 'input', 'a'],
    FORBID_ATTR: ['style'],
  }),
)
watch(
  [() => props.visible, dialog],
  ([visible]) => {
    if (visible) motion.open()
    else void motion.close()
  },
  { immediate: true, flush: 'post' },
)
watch(
  () => props.result,
  () => {
    copied.value = false
  },
)
onBeforeUnmount(() => motion.dispose())
async function copy() {
  try {
    if (isTauri()) await api.copy(props.result)
    else await navigator.clipboard.writeText(props.result)
    copied.value = true
  } catch {
    /* Copy failure must not discard the generated summary. */
  }
}
</script>

<template>
  <!-- eslint-disable vue/no-v-html -- Markdown is sanitized with DOMPurify. -->
  <Teleport to="body">
    <dialog
      id="summary-generation-dialog"
      ref="dialog"
      class="ai-dialog summary-generation-dialog"
      tabindex="-1"
      aria-labelledby="summary-generation-title"
      aria-describedby="summary-generation-description"
      @cancel.prevent="!exporting && emit('close')"
    >
      <header class="ai-dialog-heading">
        <div>
          <PhSparkle :size="23" aria-hidden="true" />
          <h2 id="summary-generation-title">AI 总结</h2>
        </div>
        <span>{{ configuring ? '确认生成' : generating ? 'AI 处理中' : '结果与导出' }}</span>
        <button
          type="button"
          class="toolbar-icon-button"
          data-window-close="default"
          aria-label="关闭 AI 总结"
          :disabled="exporting"
          @click="emit('close')"
        >
          <PhX :size="16" aria-hidden="true" />
        </button>
      </header>
      <p id="summary-generation-description" class="ai-dialog-description">
        {{
          configuring
            ? '核对日期与提示词，确认后使用默认 AI 生成总结。'
            : generating
              ? '正在整理所选日报，完成后自动显示总结。'
              : '核对生成结果，可复制内容或导出为 Markdown、TXT。'
        }}
      </p>
      <ol class="summary-workflow-steps" aria-label="AI 总结生成流程">
        <li
          v-for="(label, index) in ['确认生成', 'AI 处理', '结果导出']"
          :key="label"
          :class="{ 'is-current': step === index, 'is-complete': step > index }"
          :aria-current="step === index ? 'step' : undefined"
        >
          <span>{{ index + 1 }}</span
          >{{ label }}
        </li>
      </ol>
      <template v-if="configuring">
        <div class="summary-session-overview">
          <div>
            <span>总结日期</span><strong>{{ rangeLabel }}</strong>
          </div>
          <div>
            <span>使用 AI</span><strong>{{ provider || '尚未配置' }}</strong>
          </div>
          <div>
            <span>参与总结</span><strong>{{ reportCount }} 天有日报 · 已选择 {{ dates.length }} 天</strong>
          </div>
        </div>
        <div class="summary-generation-editor">
          <div class="summary-generation-editor-heading">
            <label for="summary-session-prompt">本次总结提示词</label><span>仅本次有效</span>
          </div>
          <textarea
            id="summary-session-prompt"
            :value="prompt"
            maxlength="2000"
            spellcheck="false"
            aria-describedby="summary-session-prompt-hint"
            :aria-invalid="!prompt.trim() || undefined"
            @input="emit('update:prompt', ($event.target as HTMLTextAreaElement).value)"
          />
          <div class="summary-generation-editor-hint">
            <p id="summary-session-prompt-hint">修改仅用于本次生成，不会同步到设置；关闭后恢复设置中的提示词。</p>
            <span>{{ prompt.length }} / 2000</span>
          </div>
          <p v-if="blockedReason" class="settings-error" role="alert">
            {{ blockedReason }}
            <button type="button" class="text-action" @click="emit('configure')">前往 AI 设置</button>
          </p>
          <p v-else-if="promptError || error" class="settings-error" role="alert">{{ promptError || error }}</p>
        </div>
      </template>
      <template v-else>
        <details class="summary-session-prompt">
          <summary>本次提示词</summary>
          <pre>{{ prompt }}</pre>
        </details>
        <div v-if="generating" class="summary-generation-progress" :aria-busy="true">
          <p class="summary-generation-scope">{{ rangeLabel }}</p>
          <div class="ai-progress" role="status" aria-live="polite">
            <div class="ai-progress-head">
              <PhCircleNotch class="spin" :size="18" aria-hidden="true" /><strong>AI 正在总结所选日报…</strong
              ><span class="ai-progress-time">已用时 {{ elapsedSeconds }} 秒</span>
            </div>
            <div class="ai-progress-track" role="progressbar" aria-label="AI 总结进度"><span /></div>
            <p class="ai-progress-hint">完成后将自动展示总结；内容较多时需要更久。</p>
          </div>
        </div>
        <div
          v-else-if="result"
          class="summary-generation-result summary-markdown"
          tabindex="0"
          aria-label="AI 总结结果"
          v-html="rendered"
        />
        <div v-else class="summary-generation-progress">
          <p class="settings-error" role="alert">{{ error || '没有生成总结，请返回修改提示词后重试。' }}</p>
        </div>
      </template>
      <p v-if="outputError" class="settings-error" role="alert">{{ outputError }}</p>
      <footer class="summary-generation-actions">
        <div class="excel-preview-directory" :title="outputDirectory">
          <PhFolderOpen :size="15" aria-hidden="true" /><span>{{ outputDirectory }}</span>
        </div>
        <div class="ai-dialog-confirm-actions">
          <button
            v-if="!configuring && !generating"
            type="button"
            class="text-action summary-edit-generation"
            :disabled="exporting"
            @click="emit('edit')"
          >
            <PhArrowClockwise :size="14" aria-hidden="true" />修改提示词并重新生成
          </button>
          <button
            v-if="!configuring && !generating && result"
            type="button"
            class="summary-result-icon"
            :disabled="exporting"
            :aria-label="copied ? '已复制' : '复制总结'"
            @click="copy"
          >
            <PhCheck v-if="copied" :size="15" /><PhCopy v-else :size="15" />
          </button>
          <md-text-button
            class="dialog-button dialog-button-secondary"
            data-window-close
            :disabled="exporting"
            @click="emit('close')"
            >{{ generating ? '取消生成' : configuring ? '取消' : '返回' }}</md-text-button
          >
          <md-filled-button
            v-if="configuring"
            class="dialog-button dialog-button-primary summary-start-generation"
            :disabled="!!blockedReason || !!promptError || exporting"
            @click="emit('generate')"
            >确认并生成</md-filled-button
          >
          <template v-else-if="!generating && result">
            <md-outlined-button class="dialog-button" :disabled="exporting" @click="emit('export', 'txt')"
              >导出 TXT</md-outlined-button
            >
            <md-filled-button
              class="dialog-button dialog-button-primary"
              :disabled="exporting"
              @click="emit('export', 'md')"
              ><PhCircleNotch v-if="exporting" slot="icon" class="spin" :size="14" />{{
                exporting ? '正在导出…' : '导出 Markdown'
              }}</md-filled-button
            >
          </template>
        </div>
      </footer>
    </dialog>
  </Teleport>
</template>
