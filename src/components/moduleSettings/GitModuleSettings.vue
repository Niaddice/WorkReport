<script setup lang="ts">
import { ref } from 'vue'
import GitSettings from '../GitSettings.vue'
import { useModuleSettingsContext } from '../../modules/settingsContext'

const context = useModuleSettingsContext()
const { draft, busy, workspace } = context
const gitForm = ref<InstanceType<typeof GitSettings>>()

function validate(): boolean {
  return gitForm.value?.validate() ?? true
}

function focusInvalid() {
  gitForm.value?.focusInvalid()
}

defineExpose({ validate, focusInvalid })
</script>

<template>
  <GitSettings
    ref="gitForm"
    :model-value="draft.git"
    :busy="busy"
    :workspace-path="workspace?.path ?? ''"
    @update:model-value="draft.git = $event"
  />
</template>
