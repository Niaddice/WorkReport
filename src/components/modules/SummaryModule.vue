<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import AiSummaryDialog from '../AiSummaryDialog.vue'
import { useModuleContext } from '../../modules/context'
import { pushMessage } from '../../lib/messages'

const context = useModuleContext()
const { journal, ai, summaryOutput, actions, disabled } = context
const { selectedDate } = journal
const {
  defaultProvider,
  summaryVisible,
  summaryGenerating,
  summaryElapsedSeconds,
  summaryDates,
  summaryScopeDates,
  summaryResult,
  summaryError,
  summaryMode,
} = ai
const { loading: outputLoading, exporting: outputExporting } = summaryOutput

const preparing = ref(false)
const prepareError = ref('')
let prepareToken = 0

/** 有框选范围时总结整个范围，否则总结当前选中的一天。 */
const scopeDates = computed(() =>
  context.rangeDates.value.length ? [...context.rangeDates.value].sort() : [selectedDate.value],
)
const scopeKey = computed(() => scopeDates.value.join(','))
const sourceKey = computed(() => JSON.stringify([journal.workspace.value?.path, scopeKey.value]))
const summaryScopeKey = computed(() => summaryScopeDates.value.join(','))
const prepared = computed(() => summaryVisible.value && summaryScopeKey.value === scopeKey.value)
const dialogDates = computed(() => (prepared.value ? summaryDates.value : scopeDates.value))
const aiConfigured = computed(() => !!defaultProvider.value)
const providerName = computed(() => defaultProvider.value?.name ?? '')
const outputError = computed(
  () => summaryOutput.exportError.value || summaryOutput.openError.value || summaryOutput.error.value,
)

/** Excel 相关事件在内嵌总结里不可用（excel-enabled 为 false），统一 no-op。 */
function noop() {}

function generateSummary() {
  if (!aiConfigured.value) {
    pushMessage({
      tone: 'info',
      text: '尚未设置默认 AI，AI 总结暂不可用。请先前往 AI 设置完成配置。',
      actions: [{ label: '前往 AI 设置', run: () => actions.openAiSettings() }],
      duration: 0,
    })
    return
  }
  actions.generateSummary()
}

async function prepareContent() {
  if (preparing.value || disabled.value || summaryGenerating.value) return
  const token = ++prepareToken
  const requestedSource = sourceKey.value
  preparing.value = true
  prepareError.value = ''
  try {
    await actions.requestSummary([...scopeDates.value])
    if (token !== prepareToken) return
  } catch (cause) {
    if (token === prepareToken) prepareError.value = `读取日报内容失败：${String(cause)}`
  } finally {
    if (token === prepareToken) {
      preparing.value = false
      // 框选在读取期间发生变化时，补读最新范围，避免沿用上一段日期的状态。
      if (requestedSource !== sourceKey.value && !prepared.value) void prepareContent()
    }
  }
}

// 打开模块或日历选择变化时自动准备原文；用户不再需要点击“准备总结”。
watch(
  [sourceKey, summaryScopeKey, summaryVisible, () => summaryGenerating.value, () => disabled.value],
  () => {
    if (prepared.value) {
      prepareError.value = ''
      preparing.value = false
      return
    }
    void prepareContent()
  },
  { immediate: true },
)
onBeforeUnmount(() => {
  prepareToken++
})

// 原文准备好后默认直接合并所选日报；AI 总结改由“总结”按钮确认后触发。
watch(
  [prepared, summaryMode, summaryGenerating],
  ([ready, mode, generating]) => {
    if (!ready || mode || generating) return
    actions.directSummary()
  },
  { immediate: true },
)
</script>

<template>
  <div class="summary-module" :aria-busy="preparing">
    <div class="summary-module-panel">
      <AiSummaryDialog
        inline
        :visible="true"
        :prepared="prepared"
        :preparing="preparing"
        :prepare-error="prepareError"
        :ai-available="aiConfigured"
        :generating="summaryGenerating"
        :elapsed-seconds="summaryElapsedSeconds"
        :dates="dialogDates"
        :result="prepared ? summaryResult : ''"
        :error="prepared ? summaryError : ''"
        :provider="providerName"
        :mode="prepared ? summaryMode : ''"
        :output-loading="outputLoading"
        :output-exporting="outputExporting"
        :output-error="outputError"
        :excel-enabled="false"
        :excel-preparing="false"
        :excel-exporting="false"
        :excel-blocked-reason="''"
        :excel-result="null"
        :excel-preview="null"
        :excel-error="''"
        @close="actions.closeSummary"
        @ai="generateSummary"
        @configure-ai="actions.openAiSettings"
        @export="actions.exportSummary"
        @excel="noop"
        @excel-unavailable="noop"
        @cancel-excel="noop"
        @confirm-excel="noop"
        @discard-excel="noop"
        @regenerate-excel="noop"
        @open-excel-directory="noop"
      />
    </div>
  </div>
</template>
