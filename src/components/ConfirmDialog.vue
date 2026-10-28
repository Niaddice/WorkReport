<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import { PhFileText } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import type { FileFormat } from '../lib/workspace'

const props = defineProps<{
  kind: 'reload' | 'format' | 'restore'
  targetFormat: FileFormat
  filename: string | undefined
  date: string
  storageMode: 'files' | 'dual' | 'database'
  error: string
  busy: boolean
}>()
const emit = defineEmits<{ confirm: []; cancel: [] }>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const title = computed(() =>
  props.kind === 'format'
    ? '切换日报文件格式？'
    : props.kind === 'restore'
      ? '恢复润色前的原文？'
      : '重新载入磁盘内容？',
)
function open() {
  motion.open()
}
function close() {
  return motion.close()
}
function isOpen() {
  return !!dialog.value?.open
}
defineExpose({ open, close, isOpen })
onBeforeUnmount(() => motion.dispose())
</script>

<template>
  <dialog
    ref="dialog"
    tabindex="-1"
    role="dialog"
    aria-modal="true"
    class="confirmation-dialog"
    aria-labelledby="dialog-title"
    aria-describedby="dialog-description"
    @cancel.prevent="!busy && emit('cancel')"
  >
    <div class="dialog-icon"><PhFileText :size="28" weight="light" /></div>
    <h2 id="dialog-title">{{ title }}</h2>
    <p v-if="kind === 'format'" id="dialog-description">
      将 <strong>{{ filename }}</strong> 切换为 <strong>{{ date }}.{{ targetFormat }}</strong
      >。<br /><template v-if="storageMode === 'database'"
        >文本内容保持原样，仅更新数据库中的日报格式，已有文件保持不变。</template
      ><template v-else>文本内容保持原样，保存后会更换文件扩展名。若目标文件已存在，将阻止切换。</template>
    </p>
    <p v-else-if="kind === 'restore'" id="dialog-description">
      将使用 AI 润色前保留的原始日报替换当前文本，并自动保存。
    </p>
    <p v-else id="dialog-description">
      当前未保存的编辑将被{{
        storageMode === 'database' ? '数据库中的内容' : '磁盘文件'
      }}替换。<br />请先复制需要保留的草稿。
    </p>
    <p v-if="error" class="error-text dialog-error" role="alert">{{ error }}</p>
    <div class="dialog-actions">
      <md-text-button
        class="dialog-button dialog-button-secondary"
        data-window-close="default"
        :disabled="busy"
        @click="emit('cancel')"
        >取消</md-text-button
      >
      <md-filled-button
        class="dialog-button dialog-button-primary"
        data-window-close
        :disabled="busy"
        @click="emit('confirm')"
        >{{
          busy ? '处理中…' : kind === 'format' ? '确认切换' : kind === 'restore' ? '恢复原文' : '重新载入'
        }}</md-filled-button
      >
    </div>
  </dialog>
</template>
