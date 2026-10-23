<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import { useAiModels } from '../lib/useAiModels'
import AppSelect from './AppSelect.vue'

const props = defineProps<{ modelValue: string; baseUrl: string; apiKey: string; disabled: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const fieldId = useId()
const manual = ref(false)
const { models, loading, error, queried, fetchModels } = useAiModels(() => ({ baseUrl: props.baseUrl, apiKey: props.apiKey }))
const currentListed = computed(() => models.value.some((model) => model.id === props.modelValue))
const modelOptions = computed(() => [
  { value: '', label: '请选择模型', disabled: true },
  ...(!props.modelValue || currentListed.value ? [] : [{ value: props.modelValue, label: `${props.modelValue}（当前模型，未在列表中）` }]),
  ...models.value.map(model => ({ value: model.id, label: model.name === model.id ? model.id : `${model.name} · ${model.id}` })),
])
async function fetch() { if (await fetchModels()) manual.value = false }
</script>

<template>
  <div class="model-picker">
    <div class="model-picker-heading"><label :for="fieldId">模型</label><button class="text-action" :disabled="disabled || loading || !baseUrl.trim()" @click="fetch"><Transition name="button-label" mode="out-in"><span :key="String(loading)">{{ loading ? '正在获取…' : '获取模型' }}</span></Transition></button></div>
    <div class="model-picker-control">
      <Transition name="control-switch" mode="out-in">
      <AppSelect v-if="models.length && !manual" key="list" :id="fieldId" :disabled="disabled" :model-value="modelValue" :options="modelOptions" @update:model-value="emit('update:modelValue', $event)" />
      <input v-else key="manual" :id="fieldId" :disabled="disabled" :value="modelValue" type="text" placeholder="获取模型列表，或手动填写模型 ID" spellcheck="false" @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)" />
      </Transition>
      <Transition name="status-fade"><button v-if="models.length" class="text-action model-entry-toggle" :disabled="disabled" @click="manual = !manual"><Transition name="button-label" mode="out-in"><span :key="String(manual)">{{ manual ? '从列表选择' : '手动填写' }}</span></Transition></button></Transition>
    </div>
    <Transition name="status-fade" mode="out-in">
      <p v-if="error" key="error" class="model-picker-error" role="alert">{{ error }}</p>
      <p v-else-if="loading" key="loading" class="field-hint" role="status">正在获取可用模型列表…</p>
      <p v-else-if="queried" key="queried" class="field-hint" role="status">{{ models.length ? `已获取 ${models.length} 个模型，选择后随 AI 设置保存。` : '接口未返回可选模型，可手动填写模型 ID。' }}</p>
    </Transition>
  </div>
</template>
