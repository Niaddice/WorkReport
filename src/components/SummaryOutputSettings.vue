<script setup lang="ts">
import { computed } from 'vue'
import { PhFolderOpen } from '@phosphor-icons/vue'
import type { SummaryOutputSettings } from '../lib/useSummaryOutput'

const props = defineProps<{
  modelValue: SummaryOutputSettings
  busy: boolean
  workspacePath: string
  directory?: string
  filenameTemplate?: string
}>()
const emit = defineEmits<{ 'update:modelValue': [settings: SummaryOutputSettings]; configureDirectories: [] }>()
const outputLocation = computed(() =>
  props.workspacePath
    ? `${props.workspacePath.replace(/[\\/]+$/, '')}/${props.directory ?? props.modelValue.outputDirectory}`
    : '请先选择工作空间',
)
defineExpose({ validate: () => true })
</script>

<template>
  <div class="summary-output-settings">
    <div class="workspace-location summary-output-location">
      <PhFolderOpen :size="20" aria-hidden="true" />
      <div>
        <strong>总结保存目录</strong><span>{{ outputLocation }}</span>
      </div>
    </div>
    <p v-if="filenameTemplate" class="field-hint">文件名规则：{{ filenameTemplate }}（导出时自动添加 .md 或 .txt）。</p>
    <p class="field-hint">保存目录和文件名规则已在“基础设置 → 目录设置”中统一管理，Markdown 和 TXT 共用相同规则。</p>
    <button type="button" class="text-action" :disabled="busy || !workspacePath" @click="emit('configureDirectories')">
      打开目录设置
    </button>
  </div>
</template>
