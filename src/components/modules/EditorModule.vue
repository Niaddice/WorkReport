<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import JournalEditor from '../JournalEditor.vue'
import WorkspaceDocumentView from '../WorkspaceDocumentView.vue'
import RangeOverview from '../RangeOverview.vue'
import { useModuleContext } from '../../modules/context'
const context = useModuleContext()
const { journal, actions, disabled, storageMode, canRestore } = context
const { rangeDates, rangeDocuments, rangeLoading, rangeError } = context
const splitPreview = ref(false)
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
        @edit="edit"
        @undo="undo"
        @redo="redo"
        @save="flush"
        @reload="actions.requestReload"
        @format="actions.requestFormat"
        @restore="actions.requestRestore"
        @error="reportError"
      />
    </Transition>
  </div>
</template>
