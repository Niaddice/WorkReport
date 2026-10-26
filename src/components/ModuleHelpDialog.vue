<script setup lang="ts">
import { onBeforeUnmount, ref, useId } from 'vue'
import { PhQuestion } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'

const props = defineProps<{
  title: string
  help: readonly string[]
}>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
const titleId = useId()

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
    class="module-help-dialog"
    :aria-labelledby="titleId"
    @cancel.prevent="close()"
  >
    <div class="dialog-icon"><PhQuestion :size="28" weight="light" /></div>
    <h2 :id="titleId">{{ props.title }} · 使用说明</h2>
    <ul class="module-help-list">
      <li v-for="(item, index) in help" :key="index">{{ item }}</li>
    </ul>
    <div class="dialog-actions">
      <md-filled-button class="dialog-button dialog-button-primary" @click="close()">知道了</md-filled-button>
    </div>
  </dialog>
</template>
