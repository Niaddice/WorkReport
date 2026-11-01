<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { PhCopy, PhNotePencil } from '@phosphor-icons/vue'
import {
  selectedCommitInformation,
  selectedCommitReport,
  groupSelectedCommits,
  type SelectedGitCommit,
} from '../lib/gitCommitSelection'
import { api } from '../lib/workspace'

const props = defineProps<{
  entries: SelectedGitCommit[]
  disabled: boolean
  appendDateLabel?: string
  appendReport?: (content: string) => Promise<boolean>
}>()
const mode = ref<'information' | 'report'>('information')
const information = computed(() => selectedCommitInformation(props.entries))
const report = ref(selectedCommitReport(props.entries))
const groups = computed(() => groupSelectedCommits(props.entries))
const copying = ref(false)
const appending = ref(false)
const message = ref('')
const error = ref('')
let revision = 0
watch(
  () => props.entries,
  () => {
    revision++
    report.value = selectedCommitReport(props.entries)
    message.value = ''
    error.value = ''
  },
)
onBeforeUnmount(() => revision++)
async function copy() {
  if (props.disabled || copying.value) return
  const current = revision
  copying.value = true
  error.value = ''
  try {
    await api.copy(mode.value === 'information' ? information.value : report.value)
    if (current === revision) message.value = '已复制'
  } catch (cause) {
    if (current === revision) error.value = `复制失败：${String(cause)}`
  } finally {
    copying.value = false
  }
}
async function append() {
  if (!props.appendReport || !props.appendDateLabel || props.disabled || appending.value || !report.value.trim()) return
  const current = revision
  appending.value = true
  error.value = ''
  try {
    const saved = await props.appendReport(report.value)
    if (current === revision) {
      if (saved) message.value = '工作记录已追加到日报'
      else error.value = '追加未完成，请检查日报保存状态或是否已包含相同内容。'
    }
  } catch (cause) {
    if (current === revision) error.value = `追加失败：${String(cause)}`
  } finally {
    appending.value = false
  }
}
</script>

<template>
  <section class="git-collection" aria-label="已获取的提交信息" :aria-busy="appending">
    <header class="collection-heading">
      <div>
        <strong>提交信息</strong><span>{{ entries.length }} 条提交 · {{ groups.length }} 个仓库</span>
      </div>
      <div class="collection-actions">
        <button type="button" :disabled="disabled || copying || !entries.length" @click="copy">
          <PhCopy :size="16" />{{ copying ? '正在复制…' : '复制' }}
        </button>
        <button
          v-if="appendReport"
          type="button"
          :disabled="disabled || appending || !appendDateLabel || !report.trim()"
          :title="appendDateLabel ? `将工作记录追加到 ${appendDateLabel} 的日报` : '在日历中选择单日后可追加到日报'"
          @click="append"
        >
          <PhNotePencil :size="16" />{{ appending ? '正在追加…' : '追加工作记录到日报' }}
        </button>
      </div>
    </header>
    <div class="collection-modes" role="group" aria-label="提交信息格式">
      <button type="button" :aria-pressed="mode === 'information'" @click="mode = 'information'">完整信息</button>
      <button type="button" :aria-pressed="mode === 'report'" @click="mode = 'report'">工作记录</button>
      <span>{{
        mode === 'information'
          ? '包含仓库、完整哈希、作者、时间及原始提交说明'
          : '可在复制或追加前编辑，保留原始工作内容'
      }}</span>
    </div>
    <p v-if="error" class="collection-feedback error" role="alert">{{ error }}</p>
    <p v-else-if="message" class="collection-feedback" role="status">{{ message }}</p>
    <textarea
      v-if="mode === 'information'"
      :value="information"
      readonly
      aria-label="完整提交信息"
      spellcheck="false"
    />
    <textarea
      v-else
      v-model="report"
      :disabled="appending || disabled"
      aria-label="选中提交的工作记录"
      spellcheck="false"
    />
    <p class="collection-note">
      {{
        appendDateLabel
          ? `工作记录追加到 ${appendDateLabel}，保留已有内容。`
          : '提交按仓库分组；每个仓库内按提交时间降序排列。'
      }}
    </p>
  </section>
</template>

<style scoped>
.git-collection {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  padding: 16px;
  gap: 12px;
  overflow: auto;
}
.collection-heading,
.collection-actions,
.collection-modes {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.collection-heading {
  justify-content: space-between;
}
.collection-heading > div:first-child {
  display: grid;
  gap: 5px;
}
.collection-heading strong {
  font-size: var(--text-subtitle);
  font-weight: 500;
  color: var(--skin-text);
}
.collection-heading span,
.collection-note,
.collection-modes > span {
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  line-height: 1.7;
}
button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 6px 9px;
  background: var(--skin-surface);
  border: 1px solid var(--skin-border);
  border-radius: 6px;
  color: var(--skin-text-body);
  font: inherit;
  cursor: pointer;
}
button:hover:not(:disabled),
button[aria-pressed='true'] {
  color: var(--skin-accent);
  background: var(--skin-accent-soft);
  border-color: var(--skin-accent-border);
}
button:disabled {
  opacity: 0.45;
  cursor: default;
}
button:focus-visible,
textarea:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
}
textarea {
  flex: 1;
  min-height: 180px;
  width: 100%;
  box-sizing: border-box;
  resize: none;
  padding: 14px;
  border: 1px solid var(--skin-border);
  border-radius: 7px;
  background: var(--skin-surface);
  color: var(--skin-text-body);
  font:
    var(--text-body) / 1.8 'SFMono-Regular',
    Consolas,
    'PingFang SC',
    monospace;
}
.collection-feedback {
  margin: 0;
  color: var(--skin-success);
  font-size: var(--text-small);
}
.collection-feedback.error {
  color: var(--skin-danger);
}
.collection-note {
  margin: 0;
}
</style>
