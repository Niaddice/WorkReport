<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import { PhGearSix } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'

const props = defineProps<{
  action: 'home' | 'workspace' | 'close' | null
  busy: boolean
  saving: boolean
  error: string
}>()
const emit = defineEmits<{ cancel: []; discard: []; save: [] }>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const target = computed(() =>
  props.action === 'close' ? '关闭窗口' : props.action === 'workspace' ? '切换工作空间' : '返回首页',
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
    role="alertdialog"
    aria-modal="true"
    class="confirmation-dialog settings-leave-dialog"
    aria-labelledby="settings-leave-title"
    aria-describedby="settings-leave-description"
    @cancel.prevent="!busy && !saving && emit('cancel')"
  >
    <div class="dialog-icon"><PhGearSix :size="28" weight="light" /></div>
    <h2 id="settings-leave-title">保存设置修改？</h2>
    <p id="settings-leave-description">设置中有未保存的修改。保存后再{{ target }}，或放弃本次修改。</p>
    <p v-if="error" class="error-text dialog-error" role="alert">{{ error }}</p>
    <div class="dialog-actions">
      <md-text-button class="dialog-button dialog-button-secondary" :disabled="busy || saving" @click="emit('cancel')"
        >继续编辑</md-text-button
      >
      <md-text-button class="dialog-button dialog-button-secondary" :disabled="busy || saving" @click="emit('discard')"
        >不保存</md-text-button
      >
      <md-filled-button class="dialog-button dialog-button-primary" :disabled="busy || saving" @click="emit('save')">{{
        busy || saving ? '正在保存…' : '保存并继续'
      }}</md-filled-button>
    </div>
  </dialog>
</template>
