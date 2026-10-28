<script setup lang="ts">
import { computed, ref } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { PhFolderOpen, PhPlus, PhTrash } from '@phosphor-icons/vue'
import { cloneGitSettings, validateGitSettings, type GitRepository, type GitSettings } from '../lib/useGit'

const props = defineProps<{ modelValue: GitSettings; busy: boolean; workspacePath: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: GitSettings] }>()
const localError = ref('')
const picking = ref(false)
const formBusy = computed(() => props.busy || picking.value)

function update(patch: Partial<GitSettings>) {
  localError.value = ''
  emit('update:modelValue', { ...cloneGitSettings(props.modelValue), ...patch })
}
function updateRepository(id: string, patch: Partial<GitRepository>) {
  update({
    repositories: props.modelValue.repositories.map((item) => (item.id === id ? { ...item, ...patch } : { ...item })),
  })
}
function add() {
  update({
    repositories: [
      ...props.modelValue.repositories,
      { id: crypto.randomUUID(), name: '', path: '', author: '', branch: '' },
    ],
  })
}
async function choose(id: string) {
  if (formBusy.value) return
  if (!isTauri()) {
    localError.value = '请在桌面应用中选择仓库，或直接填写目录绝对路径。'
    return
  }
  picking.value = true
  localError.value = ''
  const workspace = props.workspacePath
  try {
    const path = await open({ title: '选择本地 Git 仓库', directory: true, multiple: false })
    if (typeof path !== 'string' || workspace !== props.workspacePath) return
    const repository = props.modelValue.repositories.find((item) => item.id === id)
    if (repository)
      updateRepository(id, { path, name: repository.name || path.split(/[\\/]/).filter(Boolean).at(-1) || 'Git 仓库' })
  } catch (cause) {
    localError.value = `选择仓库失败：${String(cause)}`
  } finally {
    picking.value = false
  }
}
function validate() {
  localError.value = picking.value ? '请先完成仓库目录选择。' : validateGitSettings(props.modelValue)
  return !localError.value
}
defineExpose({ validate })
</script>

<template>
  <div class="git-settings">
    <label class="git-field">
      <span>Git 程序</span>
      <input
        :value="modelValue.executable"
        :disabled="formBusy"
        placeholder="git 或可执行文件绝对路径"
        @input="update({ executable: ($event.target as HTMLInputElement).value })"
      />
      <small>默认使用本机 Git；找不到程序时可填写安装路径。</small>
    </label>
    <label class="git-checkbox">
      <input
        type="checkbox"
        :checked="modelValue.includeMerges"
        :disabled="formBusy"
        @change="update({ includeMerges: ($event.target as HTMLInputElement).checked })"
      />
      包含合并提交
    </label>
    <div class="git-repositories-heading">
      <h4>
        本地仓库 <span>{{ modelValue.repositories.length }} / 20</span>
      </h4>
      <button
        class="text-action"
        type="button"
        :disabled="formBusy || modelValue.repositories.length >= 20"
        @click="add"
      >
        <PhPlus :size="16" />添加仓库
      </button>
    </div>
    <p v-if="!modelValue.repositories.length" class="git-hint">
      添加项目仓库后，即可在 Git 日报模块中获取当天的工作记录。
    </p>
    <article v-for="(repository, index) in modelValue.repositories" :key="repository.id" class="git-repository-form">
      <div class="git-repositories-heading">
        <h4>仓库 {{ index + 1 }}</h4>
        <button
          class="text-action"
          type="button"
          :aria-label="`删除仓库 ${index + 1}`"
          :disabled="formBusy"
          @click="update({ repositories: modelValue.repositories.filter((item) => item.id !== repository.id) })"
        >
          <PhTrash :size="16" />删除
        </button>
      </div>
      <label class="git-field"
        ><span>项目名称</span
        ><input
          :value="repository.name"
          :disabled="formBusy"
          maxlength="100"
          placeholder="例如：客户管理平台"
          @input="updateRepository(repository.id, { name: ($event.target as HTMLInputElement).value })"
      /></label>
      <label class="git-field"
        ><span>仓库目录</span
        ><span class="git-path-input"
          ><input
            :value="repository.path"
            :disabled="formBusy"
            placeholder="本地仓库的绝对路径"
            @input="updateRepository(repository.id, { path: ($event.target as HTMLInputElement).value })" /><button
            type="button"
            class="text-action"
            :disabled="formBusy"
            :aria-label="`选择仓库 ${index + 1} 的目录`"
            @click="choose(repository.id)"
          >
            <PhFolderOpen :size="18" /></button></span
      ></label>
      <div class="git-fields-row">
        <label class="git-field"
          ><span>作者筛选</span
          ><input
            :value="repository.author"
            :disabled="formBusy"
            placeholder="姓名或邮箱；留空为全部作者"
            @input="updateRepository(repository.id, { author: ($event.target as HTMLInputElement).value })"
        /></label>
        <label class="git-field"
          ><span>分支</span
          ><input
            :value="repository.branch"
            :disabled="formBusy"
            placeholder="留空为当前分支 HEAD"
            @input="updateRepository(repository.id, { branch: ($event.target as HTMLInputElement).value })"
        /></label>
      </div>
    </article>
    <p class="git-hint">按本机时区和提交时间筛选日期。作者按姓名或邮箱包含匹配，不区分大小写。只读取本地已有提交。</p>
    <p class="git-hint">
      保存时会检查仓库实际目录和主要远程地址，相同仓库只能添加一次。支持识别同一远程仓库常见的 SSH / HTTPS 地址写法。
    </p>
    <p v-if="localError" role="alert" class="field-error">{{ localError }}</p>
  </div>
</template>

<style scoped>
.git-settings {
  display: grid;
  gap: 20px;
}
.git-field {
  display: grid;
  gap: 8px;
  min-width: 0;
  color: var(--skin-text, #39485d);
  font-size: var(--text-body);
}
.git-field input:not([type='checkbox']) {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
  padding: 10px 12px;
  border: 1px solid #dce5f0;
  border-radius: 10px;
  background: var(--skin-surface, #fff);
  color: inherit;
  font: inherit;
}
.git-field input:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
}
.git-field small,
.git-hint {
  margin: 0;
  color: #8190a5;
  font-size: var(--text-small);
  line-height: 1.7;
}
.git-checkbox,
.git-repositories-heading,
.git-path-input,
.text-action {
  display: flex;
  align-items: center;
  gap: 8px;
}
.git-checkbox {
  font-size: var(--text-body);
}
.git-repositories-heading {
  justify-content: space-between;
}
.git-repositories-heading h4 {
  margin: 0;
  font-size: var(--text-body);
  font-weight: 500;
}
.git-repositories-heading h4 span {
  margin-left: 8px;
  color: #8190a5;
  font-size: var(--text-small);
}
.git-repository-form {
  display: grid;
  gap: 16px;
  padding: 20px;
  border: 1px solid #e5ecf5;
  border-radius: 14px;
  background: #f8fafd;
}
.git-fields-row {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}
.git-path-input input {
  flex: 1;
}
@media (max-width: 1000px) {
  .git-fields-row {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
