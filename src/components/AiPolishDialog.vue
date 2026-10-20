<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { PhSparkle, PhCircleNotch } from '@phosphor-icons/vue'
const props = defineProps<{ visible: boolean; generating: boolean; elapsedSeconds: number; original: string; result: string; error: string; provider: string }>()
const emit = defineEmits<{ close: []; regenerate: []; apply: []; restore: []; edit: [value: string] }>()
const dialog = ref<HTMLDialogElement>()
const progressText = computed(() => {
  if (props.elapsedSeconds < 3) return `正在调用 ${props.provider || 'AI'}…`
  if (props.elapsedSeconds < 12) return 'AI 正在润色日报…'
  if (props.elapsedSeconds < 30) return '正在生成，日报较长时会需要更久…'
  return '仍在等待 AI 返回，可停止后重试…'
})
const elapsedLabel = computed(() => {
  const seconds = props.elapsedSeconds
  return seconds < 60 ? `${seconds} 秒` : `${Math.floor(seconds / 60)} 分 ${String(seconds % 60).padStart(2, '0')} 秒`
})
watch(() => props.visible, async (value) => { await nextTick(); if (value && !dialog.value?.open) { dialog.value?.showModal(); dialog.value?.focus({ preventScroll: true }) } else if (!value) dialog.value?.close() })
</script>
<template>
  <Teleport to="body"><dialog ref="dialog" tabindex="-1" role="dialog" aria-modal="true" class="ai-dialog" aria-labelledby="ai-dialog-title" aria-describedby="ai-dialog-description" @cancel.prevent="emit('close')">
    <header class="ai-dialog-heading"><div><PhSparkle :size="23" /><h2 id="ai-dialog-title">润色日报</h2></div><span>{{ provider }}</span></header>
    <p id="ai-dialog-description" class="ai-dialog-description">核对润色结果，确认后替换当前日报。重新生成会始终以左侧原文为基础。</p>
    <section v-if="generating" class="ai-progress" aria-live="polite">
      <div class="ai-progress-head"><PhCircleNotch class="spin" :size="15" /><span>{{ progressText }}</span><span class="ai-progress-time" aria-hidden="true">{{ elapsedLabel }}</span></div>
      <div class="ai-progress-track" role="progressbar" aria-label="AI 润色进度" aria-valuetext="正在等待 AI 返回"><span></span></div>
      <p class="ai-progress-hint">AI 返回前可以继续等待，或点击“停止润色”。</p>
    </section>
    <div class="ai-comparison"><section><h3>原始日报</h3><textarea :value="original" readonly aria-label="原始日报" spellcheck="false"></textarea></section><section :aria-busy="generating"><h3>润色结果</h3><textarea :value="result" :disabled="generating" placeholder="润色结果将在这里显示" aria-label="AI 润色结果" spellcheck="false" @input="emit('edit', ($event.target as HTMLTextAreaElement).value)"></textarea></section></div>
    <p v-if="error" class="settings-error" role="alert">{{ error }}</p>
    <footer class="ai-dialog-actions"><div><button class="text-action" :disabled="generating" @click="emit('regenerate')">重新生成</button><button class="text-action muted-action" :disabled="generating" @click="emit('restore')">恢复原始日报</button></div><div class="ai-dialog-confirm-actions"><md-text-button class="dialog-button dialog-button-secondary" @click="emit('close')">{{ generating ? '停止润色' : '取消' }}</md-text-button><md-filled-button class="dialog-button dialog-button-primary" :disabled="generating || !result.trim() || !!error" @click="emit('apply')">确认使用</md-filled-button></div></footer>
  </dialog></Teleport>
</template>
