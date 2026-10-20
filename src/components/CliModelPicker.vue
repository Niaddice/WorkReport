<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import type { ProviderKind } from '../lib/ai'
import { useCliModels } from '../lib/useCliModels'

const props = defineProps<{ modelValue: string; kind: ProviderKind; executable: string; disabled: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const fieldId = useId()
const manual = ref(false)
const supportsListing = computed(() => props.kind === 'codex' || props.kind === 'opencode')
const { models, loading, error, queried, fetchModels } = useCliModels(() => ({ kind: props.kind, executable: props.executable }))
const currentListed = computed(() => models.value.some((model) => model.id === props.modelValue))
watch(() => [props.kind, props.executable], () => { manual.value = false }, { flush: 'sync' })
async function fetch() { if (await fetchModels()) manual.value = false }
</script>

<template>
  <div class="model-picker">
    <div class="model-picker-heading"><label :for="fieldId">模型（可选）</label><button v-if="supportsListing" class="text-action" :disabled="disabled || loading || !executable.trim()" @click="fetch">{{ loading ? '正在获取…' : '获取模型' }}</button></div>
    <div class="model-picker-control">
      <select v-if="models.length && !manual" :id="fieldId" :disabled="disabled" :value="modelValue" @change="emit('update:modelValue', ($event.target as HTMLSelectElement).value)">
        <option value="">使用 CLI 默认模型</option>
        <option v-if="modelValue && !currentListed" :value="modelValue">{{ modelValue }}（当前模型，未在列表中）</option>
        <option v-for="model in models" :key="model.id" :value="model.id">{{ model.name === model.id ? model.id : `${model.name} · ${model.id}` }}</option>
      </select>
      <input v-else :id="fieldId" :disabled="disabled" :value="modelValue" type="text" placeholder="留空使用 CLI 当前默认模型" spellcheck="false" @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)" />
      <button v-if="models.length" class="text-action model-entry-toggle" :disabled="disabled" @click="manual = !manual">{{ manual ? '从列表选择' : '手动填写' }}</button>
    </div>
    <p v-if="error" class="model-picker-error" role="alert">{{ error }}</p>
    <p v-else-if="loading" class="field-hint" role="status">正在获取 CLI 模型列表…</p>
    <p v-else-if="queried" class="field-hint" role="status">{{ models.length ? `已获取 ${models.length} 个模型，选择后随 AI 设置保存。` : 'CLI 未返回可选模型，可手动填写模型 ID。' }}</p>
    <p v-else-if="supportsListing && !executable.trim()" class="field-hint">先填写 CLI 路径，再获取模型。</p>
    <p v-else-if="!supportsListing" class="field-hint">该 CLI 不提供模型列表，请手动填写；留空使用 CLI 默认模型。</p>
  </div>
</template>
