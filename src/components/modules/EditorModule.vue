<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import JournalEditor from '../JournalEditor.vue'
import WorkspaceDocumentView from '../WorkspaceDocumentView.vue'
import RangeOverview from '../RangeOverview.vue'
import { useModuleContext } from '../../modules/context'
import { useGitJournalDraft } from '../../lib/useGitJournalDraft'
import { pushMessage } from '../../lib/messages'
const context = useModuleContext()
const { journal, actions, disabled, storageMode, canRestore } = context
const { rangeDates, rangeDocuments, rangeLoading, rangeError } = context
const splitPreview = ref(false)
const gitDraft = useGitJournalDraft()
const { sourceKey: scrollSourceKey } = context.journalPreview
const {
  document: activeDocument,
  selectedDate,
  isNewDraft,
  draft,
  dirty,
  saving,
  error,
  savedAt,
  canUndo,
  canRedo,
  edit,
  undo,
  redo,
  flush,
  reportError,
  readPreviousReport,
} = journal
const editor = ref<InstanceType<typeof JournalEditor> | InstanceType<typeof WorkspaceDocumentView>>()

function focus() {
  editor.value?.focus()
}
const handle = { focus }
function registerEditor() {
  context.editor.value = handle
}
onMounted(() => {
  registerEditor()
})
onBeforeUnmount(() => {
  if (context.editor.value?.focus === focus) context.editor.value = null
})

async function onGenerateGitDraft() {
  const path = journal.workspace.value?.path
  if (!path || disabled.value || gitDraft.generating.value) return
  if (!context.ai.defaultProvider.value) {
    pushMessage({ tone: 'warning', text: '请先在全局设置中配置并选择默认 AI。' })
    actions.openAiSettings()
    return
  }
  const date = selectedDate.value
  pushMessage({ tone: 'info', text: '正在读取 Git 提交并生成日报草稿…' })
  const result = await gitDraft.generate(path, date)
  if (!result) {
    if (gitDraft.error.value) {
      reportError(gitDraft.error.value)
      pushMessage({ tone: 'error', text: gitDraft.error.value })
    }
    return
  }

  const currentContent = draft.value.trim()
  if (!currentContent) {
    edit(result.content, true)
  } else {
    const separator = currentContent.endsWith('\n\n') ? '' : currentContent.endsWith('\n') ? '\n' : '\n\n'
    edit(
      `${currentContent}${separator}### 🤖 今日 Git 提交归纳（${result.commitCount} 条提交）\n\n${result.content}`,
      true,
    )
  }
  await flush()
  pushMessage({ tone: 'success', text: `已成功根据 ${result.commitCount} 条 Git 提交生成日报！` })
}
</script>

<template>
  <div class="editor-stage" @focusin="registerEditor">
    <Transition name="editor-switch">
      <WorkspaceDocumentView
        v-if="context.workspaceDocument?.path.value"
        key="workspace-file"
        ref="editor"
        :state="context.workspaceDocument"
        :disabled="disabled"
        @close="actions.showJournal?.()"
        @reload="actions.requestReload"
      />
      <RangeOverview
        v-else-if="rangeDates.length"
        key="range"
        v-model:preview="splitPreview"
        :dates="rangeDates"
        :documents="rangeDocuments"
        :loading="rangeLoading"
        :error="rangeError"
        :scroll-source-key="scrollSourceKey"
      />
      <JournalEditor
        v-else
        key="editor"
        ref="editor"
        v-model:preview="splitPreview"
        :date="selectedDate"
        :document="activeDocument"
        :is-new="isNewDraft"
        :content="draft"
        :dirty="dirty"
        :saving="saving"
        :disabled="disabled"
        :error="error"
        :saved-at="savedAt"
        :storage-mode="storageMode"
        :can-restore="canRestore"
        :can-undo="canUndo"
        :can-redo="canRedo"
        :scroll-source-key="scrollSourceKey"
        :load-previous-report="readPreviousReport"
        :generating-git-draft="gitDraft.generating.value"
        @edit="edit"
        @undo="undo"
        @redo="redo"
        @save="flush"
        @reload="actions.requestReload"
        @format="actions.requestFormat"
        @restore="actions.requestRestore"
        @error="reportError"
        @generate-git="onGenerateGitDraft"
      />
    </Transition>
  </div>
</template>
