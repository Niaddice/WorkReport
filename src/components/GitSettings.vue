<script setup lang="ts">
import { computed, nextTick, ref, useId } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { PhCaretRight, PhFolderOpen, PhPlus, PhTrash } from '@phosphor-icons/vue'
import {
  cloneGitSettings,
  DEFAULT_GIT_COMMIT_PROMPT,
  validateGitSettings,
  type GitRepository,
  type GitSettings,
} from '../lib/useGit'

const props = defineProps<{ modelValue: GitSettings; busy: boolean; workspacePath: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: GitSettings] }>()
const id = useId()
const root = ref<HTMLElement>()
const advanced = ref<HTMLDetailsElement>()
let invalidRepositoryId = ''
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
async function add() {
  const repositoryId = crypto.randomUUID()
  update({
    repositories: [...props.modelValue.repositories, { id: repositoryId, name: '', path: '', author: '', branch: '' }],
  })
  await nextTick()
  openRepository(repositoryId)?.querySelector<HTMLInputElement>('input')?.focus()
}
function openRepository(repositoryId: string) {
  const panel = [...(root.value?.querySelectorAll<HTMLDetailsElement>('[data-repository-id]') ?? [])].find(
    (item) => item.dataset.repositoryId === repositoryId,
  )
  if (panel) panel.open = true
  return panel
}
function focusInvalid() {
  if (localError.value.startsWith('Git 程序')) {
    if (advanced.value) advanced.value.open = true
    advanced.value?.querySelector<HTMLInputElement>('input')?.focus()
  } else if (localError.value.includes('提交说明提示词')) {
    root.value?.querySelector<HTMLTextAreaElement>('textarea')?.focus()
  } else openRepository(invalidRepositoryId)?.querySelector<HTMLInputElement>('input')?.focus()
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
  if (localError.value && !picking.value) {
    invalidRepositoryId =
      (
        props.modelValue.repositories.find(
          (repository) =>
            !!validateGitSettings({ executable: 'git', repositories: [repository], includeMerges: false }),
        ) ?? props.modelValue.repositories[0]
      )?.id ?? ''
    focusInvalid()
  }
  return !localError.value
}
defineExpose({ validate, focusInvalid })
</script>

<template>
  <div ref="root" class="git-settings">
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
      添加项目仓库后，即可在 Git 模块中整理工作记录，或打开 Git 工作台管理变更、分支与远程同步。
    </p>
    <details
      v-for="(repository, index) in modelValue.repositories"
      :key="repository.id"
      class="git-repository-form"
      :data-repository-id="repository.id"
      :open="modelValue.repositories.length === 1"
    >
      <summary>
        <PhCaretRight :size="16" aria-hidden="true" /><span
          ><strong>{{ repository.name || `仓库 ${index + 1}` }}</strong
          ><small :title="repository.path">{{ repository.path || '尚未选择仓库目录' }}</small></span
        >
      </summary>
      <div class="git-repository-body">
        <div class="git-repositories-heading">
          <span class="git-hint">仓库配置</span>
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
            ><span>日报作者筛选</span
            ><input
              :value="repository.author"
              :disabled="formBusy"
              placeholder="姓名或邮箱；留空为全部作者"
              @input="updateRepository(repository.id, { author: ($event.target as HTMLInputElement).value })"
          /></label>
          <label class="git-field"
            ><span>日报分支筛选</span
            ><input
              :value="repository.branch"
              :disabled="formBusy"
              placeholder="留空为当前分支 HEAD"
              @input="updateRepository(repository.id, { branch: ($event.target as HTMLInputElement).value })"
          /></label>
        </div>
      </div>
    </details>
    <label class="git-checkbox">
      <input
        type="checkbox"
        :checked="modelValue.includeMerges"
        :disabled="formBusy"
        @change="update({ includeMerges: ($event.target as HTMLInputElement).checked })"
      />
      包含合并提交
    </label>
    <p class="git-hint">
      日报按本机时区和提交时间筛选日期，作者按姓名或邮箱包含匹配，不区分大小写。以上筛选仅用于日报；Git
      工作台操作仓库当前检出的分支。
    </p>
    <p class="git-hint">
      保存时会检查仓库实际目录和主要远程地址，相同仓库只能添加一次。支持识别同一远程仓库常见的 SSH / HTTPS 地址写法。
    </p>
    <section class="git-settings-group" :aria-labelledby="`${id}-prompt-title`">
      <h4 :id="`${id}-prompt-title`">AI 提交说明</h4>
      <div class="git-field git-commit-prompt">
        <div class="git-prompt-heading">
          <label :for="`${id}-commit-prompt`">提交说明提示词</label
          ><button
            type="button"
            class="text-action"
            :disabled="formBusy || modelValue.commitPrompt === DEFAULT_GIT_COMMIT_PROMPT"
            @click.prevent="update({ commitPrompt: DEFAULT_GIT_COMMIT_PROMPT })"
          >
            恢复默认
          </button>
        </div>
        <textarea
          :id="`${id}-commit-prompt`"
          :value="modelValue.commitPrompt ?? DEFAULT_GIT_COMMIT_PROMPT"
          :disabled="formBusy"
          maxlength="2000"
          rows="5"
          aria-label="Git 提交说明提示词"
          placeholder="留空使用默认提示词"
          @input="update({ commitPrompt: ($event.target as HTMLTextAreaElement).value })"
        />
        <small>工作台使用默认 AI，根据已暂存的更改生成提交说明。最多 2000 字，保存设置后生效。</small>
      </div>
    </section>
    <details ref="advanced" class="git-advanced">
      <summary>高级设置<span>Git 程序路径</span></summary>
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
    </details>
    <p v-if="localError" role="alert" class="field-error">{{ localError }}</p>
  </div>
</template>

<style scoped>
.git-settings-group {
  display: grid;
  gap: 14px;
  border-top: 1px solid var(--skin-border);
  padding-top: 20px;
}
.git-settings-group h4 {
  margin: 0;
  font-size: var(--text-subtitle);
  font-weight: 500;
}
.git-advanced {
  border-top: 1px solid var(--skin-border);
  padding-top: 16px;
}
.git-advanced summary {
  cursor: pointer;
  font-size: var(--text-body);
  padding: 4px 0 12px;
}
.git-advanced summary span {
  margin-left: 10px;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
}
.git-settings {
  display: grid;
  gap: 20px;
}
.git-field {
  display: grid;
  gap: 8px;
  min-width: 0;
  color: var(--skin-text);
  font-size: var(--text-body);
}
.git-field input:not([type='checkbox']),
.git-field textarea {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
  padding: 10px 12px;
  border: 1px solid var(--skin-border);
  border-radius: 10px;
  background: var(--skin-surface);
  color: inherit;
  font: inherit;
}
.git-field input:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
}
.git-field textarea {
  resize: vertical;
  line-height: 1.7;
}
.git-field textarea:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
}
.git-field small,
.git-hint {
  margin: 0;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  line-height: 1.7;
}
.git-checkbox,
.git-prompt-heading,
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
.git-repositories-heading,
.git-prompt-heading {
  justify-content: space-between;
}
.git-repositories-heading h4 {
  margin: 0;
  font-size: var(--text-body);
  font-weight: 500;
}
.git-repositories-heading h4 span {
  margin-left: 8px;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
}
.git-repository-form {
  min-width: 0;
  border: 1px solid var(--skin-border);
  border-radius: 14px;
  background: var(--skin-canvas);
}
.git-repository-form > summary {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 16px;
  cursor: pointer;
  list-style: none;
}
.git-repository-form > summary::-webkit-details-marker {
  display: none;
}
.git-repository-form > summary > svg {
  flex-shrink: 0;
  transition: transform var(--motion-fast);
}
.git-repository-form[open] > summary > svg {
  transform: rotate(90deg);
}
.git-repository-form > summary > span {
  display: grid;
  gap: 5px;
  min-width: 0;
}
.git-repository-form summary strong {
  font-size: var(--text-body);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.git-repository-form summary small {
  font-size: var(--text-small);
  color: var(--skin-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.git-repository-form > summary:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
  border-radius: 12px;
}
.git-repository-body {
  display: grid;
  gap: 16px;
  padding: 0 16px 16px;
}
.git-repository-form:not([open]) > .git-repository-body {
  display: none;
}
.git-repository-body > .git-repositories-heading {
  justify-content: space-between;
}
@media (prefers-reduced-motion: reduce) {
  .git-repository-form > summary > svg {
    transition: none;
  }
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
