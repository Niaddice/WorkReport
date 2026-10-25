<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { PhGearSix } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'

defineProps<{ sourceName: string; targetPath: string; busy: boolean; error: string }>()
const emit = defineEmits<{ cancel: []; keep: []; sync: [] }>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
defineExpose({ open: motion.open, close: motion.close })
onBeforeUnmount(() => motion.dispose())
</script>

<template>
  <dialog
    ref="dialog"
    tabindex="-1"
    role="alertdialog"
    aria-modal="true"
    class="confirmation-dialog workspace-switch-dialog"
    aria-labelledby="workspace-switch-title"
    aria-describedby="workspace-switch-description"
    @cancel.prevent="!busy && emit('cancel')"
  >
    <div class="dialog-icon"><PhGearSix :size="28" weight="light" /></div>
    <h2 id="workspace-switch-title">将设置同步到新工作空间？</h2>
    <p id="workspace-switch-description">
      将「{{ sourceName }}」已保存的字体、AI
      配置与提示词、输出、数据存储、布局和皮肤设置复制到新工作空间，覆盖其对应设置。
      日报不会随设置复制，两个工作空间仍独立保存各自的日报。选择“仅切换”则使用新工作空间自己的设置。
    </p>
    <p class="workspace-switch-path" :title="targetPath">{{ targetPath }}</p>
    <p v-if="error" class="error-text dialog-error" role="alert">{{ error }}</p>
    <div class="dialog-actions">
      <md-text-button class="dialog-button dialog-button-secondary" :disabled="busy" @click="emit('cancel')"
        >取消</md-text-button
      >
      <md-text-button class="dialog-button dialog-button-secondary" :disabled="busy" @click="emit('keep')"
        >仅切换</md-text-button
      >
      <md-filled-button class="dialog-button dialog-button-primary" :disabled="busy" @click="emit('sync')">{{
        busy ? '正在切换…' : '同步并切换'
      }}</md-filled-button>
    </div>
  </dialog>
</template>
