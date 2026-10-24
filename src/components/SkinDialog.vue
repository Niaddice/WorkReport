<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { PhCheck, PhPalette } from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import { SKINS, type SkinId } from '../lib/skins'

const props = defineProps<{ active: SkinId }>()
const emit = defineEmits<{ select: [skin: SkinId]; close: [] }>()
const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)
function open() {
  motion.open()
}
function close() {
  return motion.close()
}
function isOpen() {
  return !!dialog.value?.open
}
function choose(id: SkinId) {
  emit('select', id)
}
defineExpose({ open, close, isOpen })
onBeforeUnmount(() => motion.dispose())
</script>

<template>
  <dialog ref="dialog" class="skin-dialog" aria-labelledby="skin-dialog-title" @cancel.prevent="emit('close')">
    <header class="skin-dialog-heading">
      <div>
        <PhPalette :size="22" aria-hidden="true" />
        <h2 id="skin-dialog-title">切换皮肤</h2>
      </div>
      <p>选择后立即应用到当前工作空间，无需保存设置。</p>
    </header>
    <div class="skin-grid">
      <button
        v-for="skin in SKINS"
        :key="skin.id"
        type="button"
        class="skin-card"
        :class="{ active: skin.id === props.active }"
        :aria-pressed="skin.id === props.active"
        @click="choose(skin.id)"
      >
        <span
          class="skin-preview"
          :style="{
            '--preview-canvas': skin.swatches[0],
            '--preview-surface': skin.swatches[1],
            '--preview-accent': skin.swatches[2],
          }"
        >
          <span class="skin-preview-bar"></span>
          <span class="skin-preview-body"><i></i><i></i></span>
        </span>
        <span class="skin-copy"
          ><strong>{{ skin.name }}</strong
          ><small>{{ skin.description }}</small></span
        >
        <PhCheck v-if="skin.id === props.active" class="skin-check" :size="16" aria-hidden="true" />
      </button>
      <div class="skin-card skin-card-muted" aria-hidden="true">
        <span class="skin-preview skin-preview-empty"></span>
        <span class="skin-copy"><strong>更多皮肤</strong><small>后续版本加入</small></span>
      </div>
    </div>
    <footer class="skin-dialog-actions">
      <button type="button" class="text-action" @click="emit('close')">关闭</button>
    </footer>
  </dialog>
</template>
