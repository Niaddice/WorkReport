<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import JournalEditor from '../JournalEditor.vue'
import RangeOverview from '../RangeOverview.vue'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal, actions, disabled, storageMode, canRestore } = context
const { editorPreview, rangeDates, rangeDocuments, rangeLoading, rangeError } = context
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
const editor = ref<InstanceType<typeof JournalEditor>>()

function focus() {
  editor.value?.focus()
}

onMounted(() => {
  context.editor.value = { focus }
})
onBeforeUnmount(() => {
  context.editor.value = null
})
</script>

<template>
  <div class="editor-stage">
    <Transition name="editor-switch">
      <RangeOverview
        v-if="rangeDates.length"
        key="range"
        v-model:preview="editorPreview"
        :dates="rangeDates"
        :documents="rangeDocuments"
        :loading="rangeLoading"
        :error="rangeError"
      />
      <JournalEditor
        v-else
        key="editor"
        ref="editor"
        v-model:preview="editorPreview"
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
