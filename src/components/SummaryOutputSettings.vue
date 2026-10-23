<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { PhFolderOpen } from '@phosphor-icons/vue'
import { type SummaryOutputSettings } from '../lib/useSummaryOutput'

const props = defineProps<{ modelValue: SummaryOutputSettings; busy: boolean; workspacePath: string }>()
const emit = defineEmits<{ 'update:modelValue': [settings: SummaryOutputSettings] }>()
const draft = computed(() => props.modelValue)
const picking = ref(false)
const localError = ref('')
const formBusy = computed(() => props.busy || picking.value)
const outputLocation = computed(() => {
  if (!props.workspacePath) return '请先选择工作空间'
  const base = props.workspacePath.replace(/[\\/]+$/, '')
  const relative = draft.value.outputDirectory.trim().replace(/\\/g, '/')
  return `${base}/${relative}`
})
watch(() => props.workspacePath, () => { localError.value = '' })
watch(() => props.modelValue, () => { localError.value = '' })

function updateDirectory(outputDirectory: string) {
  if (props.busy) return
  localError.value = ''
  emit('update:modelValue', { outputDirectory })
}

function normalizedDirectory(): string | null {
  const value = draft.value.outputDirectory.trim().replace(/\\/g, '/')
  if (!value || value.startsWith('/') || /^[a-z]:/i.test(value) || /[\x00-\x1f\x7f]/.test(value) || value.split('/').some(part => !part || part === '.' || part === '..')) {
    localError.value = '请输入工作空间内的相对目录，例如“总结”或“总结/周报”，不能使用根目录或上级目录。'
    return null
  }
  return value
}

function validate(): boolean {
  localError.value = ''
  if (picking.value) { localError.value = '请先完成总结输出目录选择。'; return false }
  if (!props.workspacePath) { localError.value = '请先选择工作空间，再配置总结输出目录。'; return false }
  const directory = normalizedDirectory()
  if (directory === null) return false
  if (directory !== draft.value.outputDirectory) updateDirectory(directory)
  return true
}

async function chooseDirectory() {
  if (formBusy.value || !props.workspacePath) return
  if (!isTauri()) { localError.value = '请在桌面应用中选择目录，或直接输入工作空间内的相对目录。'; return }
  picking.value = true; localError.value = ''
  const workspacePath = props.workspacePath
  try {
    const path = await open({ title: '选择工作空间内的总结输出目录', multiple: false, directory: true, defaultPath: workspacePath })
    if (typeof path !== 'string' || props.workspacePath !== workspacePath) return
    const base = workspacePath.replace(/\\/g, '/').replace(/\/+$/, '')
    const target = path.replace(/\\/g, '/').replace(/\/+$/, '')
    const windows = /^[a-z]:/i.test(base)
    const matchBase = windows ? base.toLowerCase() : base
    const matchTarget = windows ? target.toLowerCase() : target
    if (!matchTarget.startsWith(`${matchBase}/`)) {
      localError.value = '请选择当前工作空间内的子目录，不能选择工作空间根目录或外部目录。'
      return
    }
    updateDirectory(target.slice(base.length + 1))
  } catch (cause) { localError.value = `选择总结输出目录失败：${String(cause)}` }
  finally { picking.value = false }
}

function restoreDefault() {
  if (formBusy.value || !props.workspacePath) return
  updateDirectory('总结')
}

defineExpose({ validate })
</script>

<template>
  <div class="summary-output-settings">
    <div class="excel-path-field">
      <label for="summary-output-directory">总结文件输出目录</label>
      <div class="excel-path-control">
        <input id="summary-output-directory" :value="draft.outputDirectory" @input="updateDirectory(($event.target as HTMLInputElement).value)" :disabled="formBusy || !workspacePath" type="text" placeholder="总结" spellcheck="false" aria-describedby="summary-output-directory-hint" />
        <button type="button" class="text-action" :disabled="formBusy || !workspacePath" @click="chooseDirectory"><PhFolderOpen :size="17" aria-hidden="true" />选择目录</button>
      </div>
      <p id="summary-output-directory-hint" class="field-hint">填写工作空间内的相对目录，例如“总结/周报”。导出的 Markdown 和 TXT 文件保存在此处，目录不存在时会自动创建。</p>
    </div>
    <div class="workspace-location summary-output-location"><PhFolderOpen :size="20" aria-hidden="true" /><div><strong>实际保存位置</strong><span>{{ outputLocation }}</span></div></div>
    <div class="ai-settings-footer"><span>目录随工作空间保存。</span><button type="button" class="text-action" :disabled="formBusy || !workspacePath || draft.outputDirectory === '总结'" @click="restoreDefault">恢复默认目录</button></div>
    <Transition name="status-fade" mode="out-in">
      <p v-if="localError" key="error" class="settings-error" role="alert">{{ localError }}</p>
      <p v-else-if="!workspacePath" key="workspace" class="settings-muted">请先选择工作空间，再配置总结输出目录。</p>
    </Transition>
  </div>
</template>
