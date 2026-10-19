<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { PhSparkle, PhCircleNotch } from '@phosphor-icons/vue'
const props = defineProps<{ visible: boolean; generating: boolean; original: string; result: string; error: string; provider: string }>()
const emit = defineEmits<{ close: []; regenerate: []; apply: []; restore: []; edit: [value: string] }>()
const dialog = ref<HTMLDialogElement>()
watch(() => props.visible, async (value) => { await nextTick(); if (value && !dialog.value?.open) { dialog.value?.showModal(); dialog.value?.focus({ preventScroll: true }) } else if (!value) dialog.value?.close() })
</script>
<template>
  <Teleport to="body"><dialog ref="dialog" tabindex="-1" role="dialog" aria-modal="true" class="ai-dialog" aria-labelledby="ai-dialog-title" @cancel.prevent="emit('close')">
    <header class="ai-dialog-heading"><div><PhSparkle :size="23" /><h2 id="ai-dialog-title">润色日报</h2></div><span>{{ provider }}</span></header>
    <p class="ai-dialog-description">核对润色结果，确认后替换当前日报。重新生成会始终以左侧原文为基础。</p>
    <div class="ai-comparison"><section><h3>原始日报</h3><textarea :value="original" readonly aria-label="原始日报" spellcheck="false"></textarea></section><section :aria-busy="generating"><h3>润色结果<span v-if="generating"><PhCircleNotch class="spin" :size="14" />正在润色…</span></h3><textarea :value="result" :disabled="generating" placeholder="润色结果将在这里显示" aria-label="AI 润色结果" spellcheck="false" @input="emit('edit', ($event.target as HTMLTextAreaElement).value)"></textarea></section></div>
    <p v-if="error" class="settings-error" role="alert">{{ error }}</p>
    <footer class="ai-dialog-actions"><div><button class="text-action" :disabled="generating" @click="emit('regenerate')">重新生成</button><button class="text-action muted-action" @click="emit('restore')">恢复原始日报</button></div><div class="ai-dialog-confirm-actions"><md-text-button class="dialog-button dialog-button-secondary" @click="emit('close')">取消</md-text-button><md-filled-button class="dialog-button dialog-button-primary" :disabled="generating || !result.trim() || !!error" @click="emit('apply')">确认使用</md-filled-button></div></footer>
  </dialog></Teleport>
</template>
