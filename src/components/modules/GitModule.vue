<script setup lang="ts">
import { computed, ref } from 'vue'
import dayjs from 'dayjs'
import { useModuleContext } from '../../modules/context'
import GitWorkbench from '../GitWorkbench.vue'

const { git, journal, actions, disabled, rangeDates, ai } = useModuleContext()
const applying = ref(false)
const unavailable = computed(() => disabled.value || git.loading.value || applying.value)
const selectedDateLabel = computed(() => dayjs(journal.selectedDate.value).format('YYYY年M月D日'))

async function appendSelectedReport(content: string): Promise<boolean> {
  const path = journal.workspace.value?.path
  if (!path || unavailable.value || rangeDates.value.length || journal.saving.value) return false
  const date = journal.selectedDate.value
  applying.value = true
  try {
    return await actions.appendGitReport(path, date, content)
  } finally {
    applying.value = false
  }
}
</script>

<template>
  <div class="git-module">
    <p v-if="git.error.value" class="field-error" role="alert">{{ git.error.value }}</p>
    <GitWorkbench
      :workspace-path="journal.workspace.value?.path ?? ''"
      :repositories="git.settings.value.repositories"
      :disabled="unavailable"
      :ai-provider-name="ai.defaultProvider.value?.name ?? null"
      :append-date-label="rangeDates.length ? undefined : selectedDateLabel"
      :append-report="appendSelectedReport"
      @configure-ai="actions.openAiSettings"
      @configure-repositories="actions.openGitSettings"
    />
  </div>
</template>

<style scoped>
.git-module {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: var(--skin-text);
}
.field-error {
  margin: 8px 12px;
}
</style>
