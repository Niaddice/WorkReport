<script setup lang="ts">
import { ref, watch } from 'vue'
import { PhCircleNotch, PhX } from '@phosphor-icons/vue'
import ExcelSheetPreview from './ExcelSheetPreview.vue'
import type { ExcelSheetPreviewData } from '../lib/excelPreview'

const props = defineProps<{
  visible: boolean
  filename: string
  sheets: ExcelSheetPreviewData[]
  loading: boolean
  error: string
}>()
const emit = defineEmits<{ close: [] }>()
const dialog = ref<HTMLDialogElement>()

watch(
  () => props.visible,
  (visible) => {
    const element = dialog.value
    if (!element) return
    if (visible) {
      if (!element.open) element.showModal()
    } else if (element.open) {
      element.close()
    }
  },
  { flush: 'post' },
)

function onCancel(event: Event) {
  event.preventDefault()
  emit('close')
}
</script>

<template>
  <dialog ref="dialog" class="excel-export-preview-dialog" aria-label="已导出 Excel 预览" @cancel="onCancel">
    <header class="excel-export-preview-header">
      <strong :title="filename">{{ filename || 'Excel 预览' }}</strong>
      <button
        type="button"
        class="excel-export-preview-close"
        aria-label="关闭预览"
        title="关闭预览"
        @click="emit('close')"
      >
        <PhX :size="16" aria-hidden="true" />
      </button>
    </header>
    <div class="excel-export-preview-body">
      <div v-if="loading" class="excel-export-preview-state">
        <PhCircleNotch class="spin" :size="18" aria-hidden="true" /><span>正在读取 Excel 内容…</span>
      </div>
      <div v-else-if="error" class="excel-export-preview-state error-text" role="alert">{{ error }}</div>
      <ExcelSheetPreview v-else-if="sheets.length" :sheets="sheets" :preview-id="filename" />
      <div v-else class="excel-export-preview-state">没有可显示的表格内容。</div>
    </div>
  </dialog>
</template>
