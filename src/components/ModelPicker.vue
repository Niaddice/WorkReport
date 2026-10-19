<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import { useAiModels } from '../lib/useAiModels'

const props = defineProps<{ modelValue: string; baseUrl: string; apiKey: string; disabled: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const fieldId = useId()
const manual = ref(false)
const { models, loading, error, queried, fetchModels } = useAiModels(() => ({ baseUrl: props.baseUrl, apiKey: props.apiKey }))
const currentListed = computed(() => models.value.some((model) => model.id === props.modelValue))
async function fetch() { if (await fetchModels()) manual.value = false }
</script>

<template>
  <div class="model-picker">
    <div class="model-picker-heading"><label :for="fieldId">模型</label><button class="text-action" :disabled="disabled || loading || !baseUrl.trim()" @click="fetch">{{ loading ? '正在获取…' : '获取模型' }}</button></div>
    <div class="model-picker-control">
      <select v-if="models.length && !manual" :id="fieldId" :disabled="disabled" :value="modelValue" @change="emit('update:modelValue', ($event.target as HTMLSelectElement).value)">
        <option value="" disabled>请选择模型</option>
        <option v-if="modelValue && !currentListed" :value="modelValue">{{ modelValue }}（当前模型，未在列表中）</option>
        <option v-for="model in models" :key="model.id" :value="model.id">{{ model.name === model.id ? model.id : `${model.name} · ${model.id}` }}</option>
      </select>
      <input v-else :id="fieldId" :disabled="disabled" :value="modelValue" type="text" placeholder="获取模型列表，或手动填写模型 ID" spellcheck="false" @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)" />
      <button v-if="models.length" class="text-action model-entry-toggle" :disabled="disabled" @click="manual = !manual">{{ manual ? '从列表选择' : '手动填写' }}</button>
    </div>
    <p v-if="error" class="model-picker-error" role="alert">{{ error }}</p>
    <p v-else-if="loading" class="field-hint" role="status">正在获取可用模型列表…</p>
    <p v-else-if="queried" class="field-hint" role="status">{{ models.length ? `已获取 ${models.length} 个模型，选择后随 AI 设置保存。` : '接口未返回可选模型，可手动填写模型 ID。' }}</p>
  </div>
</template>
