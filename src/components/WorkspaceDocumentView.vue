<script setup lang="ts">
import { computed, ref } from 'vue'
import { PhArrowClockwise, PhArrowUUpLeft, PhCircleNotch, PhFileText } from '@phosphor-icons/vue'
import JournalEditor from './JournalEditor.vue'
import ExcelSheetPreview from './ExcelSheetPreview.vue'
import type { useWorkspaceDocument } from '../lib/useWorkspaceDocument'
import type { Document } from '../lib/workspace'
import { pushMessage } from '../lib/messages'

const props = defineProps<{ state: ReturnType<typeof useWorkspaceDocument>; disabled: boolean }>()
const emit = defineEmits<{ close: []; reload: [] }>()
const preview = ref(false)
const editor = ref<InstanceType<typeof JournalEditor>>()
const file = computed(() => props.state.document.value)
const textDocument = computed<Document | null>(() =>
  file.value?.kind === 'text'
    ? {
        date: file.value.path,
        filename: file.value.filename,
        content: file.value.content,
        revision: file.value.revision,
        format: file.value.markdown ? 'md' : 'txt',
      }
    : null,
)
function reportError(error: string) {
  pushMessage({ tone: 'error', text: error })
}
defineExpose({ focus: () => editor.value?.focus() })
</script>

<template>
  <section
    :class="{ 'is-code-document': file?.kind === 'text' && !file.markdown }"
    class="workspace-document-view"
    aria-label="目录文件查看与编辑"
  >
    <header class="workspace-document-heading">
      <div>
        <strong>{{ file?.filename ?? state.path.value.split('/').pop() }}</strong
        ><span :title="state.path.value">{{ state.path.value }}</span>
      </div>
      <span v-if="file?.kind === 'table'" class="workspace-document-mode">只读预览</span>
      <span v-else-if="file?.kind === 'text'" class="workspace-document-mode">{{ file.encoding.toUpperCase() }}</span>
      <button
        type="button"
        class="text-action"
        aria-label="重新载入文件"
        title="重新载入文件"
        :disabled="disabled || state.loading.value || state.saving.value"
        @click="emit('reload')"
      >
        <PhArrowClockwise :size="16" />
      </button>
      <button
        type="button"
        class="text-action"
        aria-label="返回日报"
        title="返回日报"
        :disabled="disabled || state.saving.value"
        @click="emit('close')"
      >
        <PhArrowUUpLeft :size="17" />
      </button>
    </header>
    <div v-if="state.loading.value" class="workspace-document-state" role="status">
      <PhCircleNotch class="spin" :size="21" />
      <p>正在打开文件…</p>
    </div>
    <JournalEditor
      v-else-if="textDocument"
      ref="editor"
      :key="state.path.value"
      v-model:preview="preview"
      :date="state.path.value"
      :document="textDocument"
      :file-path="state.path.value"
      :content="state.draft.value"
      :dirty="state.dirty.value"
      :saving="state.saving.value"
      :disabled="disabled"
      :error="state.error.value"
      :is-new="false"
      :saved-at="null"
      storage-mode="files"
      :can-restore="false"
      :can-undo="state.canUndo.value"
      :can-redo="state.canRedo.value"
      :scroll-source-key="`workspace-file:${state.path.value}`"
      @edit="state.edit"
      @save="state.flush"
      @undo="state.undo"
      @redo="state.redo"
      @reload="emit('reload')"
      @error="reportError"
    />
    <template v-else-if="file?.kind === 'table' && !state.error.value">
      <p v-if="file.message" class="workspace-document-notice">{{ file.message }}</p>
      <ExcelSheetPreview :sheets="file.sheets" :preview-id="state.path.value" :show-changes="false" />
    </template>
    <div v-else class="workspace-document-state" :role="state.error.value ? 'alert' : 'status'">
      <PhFileText :size="34" weight="light" />
      <h3>{{ state.error.value ? '无法打开文件' : '暂不支持此文件格式' }}</h3>
      <p>{{ state.error.value || file?.message || '请选择可编辑的文本文件或只读预览的 Excel、CSV 表格。' }}</p>
    </div>
  </section>
</template>
