<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import type { ProviderKind } from '../lib/ai'
import { useCliModels } from '../lib/useCliModels'
import AppSelect from './AppSelect.vue'

const props = defineProps<{ modelValue: string; kind: ProviderKind; executable: string; disabled: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const fieldId = useId()
const manual = ref(false)
const supportsListing = computed(() => props.kind === 'codex' || props.kind === 'opencode')
const { models, loading, error, queried, fetchModels } = useCliModels(() => ({ kind: props.kind, executable: props.executable }))
const currentListed = computed(() => models.value.some((model) => model.id === props.modelValue))
const modelOptions = computed(() => [
  { value: '', label: '使用 CLI 默认模型' },
  ...(!props.modelValue || currentListed.value ? [] : [{ value: props.modelValue, label: `${props.modelValue}（当前模型，未在列表中）` }]),
  ...models.value.map(model => ({ value: model.id, label: model.name === model.id ? model.id : `${model.name} · ${model.id}` })),
])
watch(() => [props.kind, props.executable], () => { manual.value = false }, { flush: 'sync' })
async function fetch() { if (await fetchModels()) manual.value = false }
</script>

<template>
  <div class="model-picker">
    <div class="model-picker-heading"><label :for="fieldId">模型（可选）</label><Transition name="status-fade"><button v-if="supportsListing" class="text-action" :disabled="disabled || loading || !executable.trim()" @click="fetch"><Transition name="button-label" mode="out-in"><span :key="String(loading)">{{ loading ? '正在获取…' : '获取模型' }}</span></Transition></button></Transition></div>
    <div class="model-picker-control">
      <Transition name="control-switch" mode="out-in">
      <AppSelect v-if="models.length && !manual" key="list" :id="fieldId" :disabled="disabled" :model-value="modelValue" :options="modelOptions" @update:model-value="emit('update:modelValue', $event)" />
      <input v-else key="manual" :id="fieldId" :disabled="disabled" :value="modelValue" type="text" placeholder="留空使用 CLI 当前默认模型" spellcheck="false" @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)" />
      </Transition>
      <Transition name="status-fade"><button v-if="models.length" class="text-action model-entry-toggle" :disabled="disabled" @click="manual = !manual"><Transition name="button-label" mode="out-in"><span :key="String(manual)">{{ manual ? '从列表选择' : '手动填写' }}</span></Transition></button></Transition>
    </div>
    <Transition name="status-fade" mode="out-in">
      <p v-if="error" key="error" class="model-picker-error" role="alert">{{ error }}</p>
      <p v-else-if="loading" key="loading" class="field-hint" role="status">正在获取 CLI 模型列表…</p>
      <p v-else-if="queried" key="queried" class="field-hint" role="status">{{ models.length ? `已获取 ${models.length} 个模型，选择后随 AI 设置保存。` : 'CLI 未返回可选模型，可手动填写模型 ID。' }}</p>
      <p v-else-if="supportsListing && !executable.trim()" key="path-required" class="field-hint">先填写 CLI 路径，再获取模型。</p>
      <p v-else-if="!supportsListing" key="manual-only" class="field-hint">该 CLI 不提供模型列表，请手动填写；留空使用 CLI 默认模型。</p>
    </Transition>
  </div>
</template>
