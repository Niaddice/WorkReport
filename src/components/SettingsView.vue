<script setup lang="ts">
import { ref } from 'vue'
import { PhArrowLeft, PhFolderOpen } from '@phosphor-icons/vue'
import { aiApi, type AiProvider, type AiSettings, type DetectedCli } from '../lib/ai'
import type { Workspace } from '../lib/workspace'
import { useAiSettingsDraft } from '../lib/useAiSettingsDraft'
import ModelPicker from './ModelPicker.vue'

const props = defineProps<{ workspace: Workspace | null; picking: boolean; settings: AiSettings; saving: boolean; error: string; saved: boolean; save: (settings: AiSettings) => Promise<boolean> }>()
const emit = defineEmits<{ back: []; workspace: [] }>()
const { draft, unchanged, flush, selectDefault } = useAiSettingsDraft(() => props.settings, (settings) => props.save(settings))
defineExpose({ flush, unchanged })
const detected = ref<DetectedCli[]>([])
const detecting = ref(false)
const detectedOnce = ref(false)
const localError = ref('')
const editingId = ref('')
const preset = ref('deepseek')
async function detect() {
  detecting.value = true; localError.value = ''
  try { detected.value = await aiApi.detect(); detectedOnce.value = true }
  catch (cause) { localError.value = String(cause) }
  finally { detecting.value = false }
}
function addCli(cli: DetectedCli) {
  const existing = draft.value.providers.find((provider) => provider.kind === cli.kind && provider.executable === cli.executable)
  if (existing) { editingId.value = existing.id; return }
  const provider: AiProvider = { id: crypto.randomUUID(), name: cli.name, kind: cli.kind, executable: cli.executable, model: '', baseUrl: '', apiKey: '' }
  draft.value.providers.push(provider); editingId.value = provider.id
}
function addCustom() {
  const provider: AiProvider = { id: crypto.randomUUID(), name: preset.value === 'deepseek' ? 'DeepSeek' : preset.value === 'gemini' ? 'Gemini' : '自定义 AI', kind: 'http', executable: '', model: preset.value === 'deepseek' ? 'deepseek-chat' : '', baseUrl: preset.value === 'deepseek' ? 'https://api.deepseek.com/v1' : preset.value === 'gemini' ? 'https://generativelanguage.googleapis.com/v1beta/openai' : '', apiKey: '' }
  draft.value.providers.push(provider); editingId.value = provider.id
}
function remove(id: string) {
  draft.value.providers = draft.value.providers.filter((provider) => provider.id !== id)
  if (draft.value.defaultProviderId === id) draft.value.defaultProviderId = null
}
</script>

<template>
  <main class="settings-page">
    <button class="text-action settings-back" @click="emit('back')"><PhArrowLeft :size="16" />返回首页</button>
    <div class="settings-intro"><h1>设置</h1><p>管理工作空间与日报润色使用的 AI。</p></div>
    <section class="settings-section" aria-labelledby="workspace-settings-title">
      <div class="settings-section-heading"><div><h2 id="workspace-settings-title">工作空间</h2><p>日报文件保存的位置</p></div><button class="text-action" :disabled="picking" @click="emit('workspace')">{{ picking ? '正在打开…' : '切换工作空间' }}</button></div>
      <div class="workspace-location"><PhFolderOpen :size="24" /><div><strong>{{ workspace?.name ?? '尚未选择工作空间' }}</strong><span>{{ workspace?.path ?? '请先选择一个本地文件夹' }}</span></div></div>
    </section>
    <section class="settings-section" aria-labelledby="ai-settings-title">
      <div class="settings-section-heading"><div><h2 id="ai-settings-title">AI 润色</h2><p>手动选择默认 AI，选择后立即保存。</p></div><button class="text-action" :disabled="detecting || saving" @click="detect">{{ detecting ? '正在检测…' : '检测本机 CLI' }}</button></div>
      <div class="generation-preference"><label for="generation-preference">AI 生成倾向</label><p id="generation-preference-hint">描述希望的语气、篇幅和侧重点，对所有 AI 生效。留空时按原意自然润色。</p><textarea id="generation-preference" v-model="draft.generationPreference" :disabled="saving" maxlength="2000" rows="3" aria-describedby="generation-preference-hint" placeholder="例如：简洁客观，突出已完成事项和下一步计划，保留原有条目结构。" spellcheck="false"></textarea><span class="preference-count">{{ draft.generationPreference.length }} / 2000</span></div>
      <div v-if="detectedOnce" class="detected-clis">
        <p v-if="!detected.length" class="settings-muted">未检测到 Codex、Claude Code 或 OpenCode。可先安装并登录 CLI，或在下方添加模型接口。</p>
        <article v-for="cli in detected" :key="cli.executable" class="detected-row"><div><strong>{{ cli.name }}<small>{{ cli.version }}</small></strong><span>{{ cli.executable }}</span></div><button class="text-action" :disabled="saving" @click="addCli(cli)">{{ draft.providers.some((provider) => provider.executable === cli.executable) ? '已添加' : '添加' }}</button></article>
      </div>
      <div class="custom-ai-add"><label for="ai-preset">添加模型接口</label><select id="ai-preset" v-model="preset" :disabled="saving"><option value="deepseek">DeepSeek</option><option value="gemini">Gemini</option><option value="custom">自定义兼容接口</option></select><button class="text-action" :disabled="saving" @click="addCustom">添加</button></div>
      <p v-if="!draft.providers.length" class="settings-empty">尚未配置 AI。检测本机 CLI，或添加一个模型接口。</p>
      <div v-else class="provider-list">
        <article v-for="provider in draft.providers" :key="provider.id" class="provider-card" :class="{ 'provider-default': settings.defaultProviderId === provider.id }">
          <div class="provider-heading"><label class="provider-choice"><input :disabled="saving" :checked="draft.defaultProviderId === provider.id" type="radio" name="default-ai" :value="provider.id" @change="selectDefault(provider.id)" /><span>{{ provider.name || '未命名 AI' }}<small>{{ provider.kind === 'http' ? provider.model || '模型接口' : '本机 CLI' }}</small></span></label><div class="provider-actions"><span v-if="settings.defaultProviderId === provider.id" class="default-label">默认 AI</span><button class="text-action" @click="editingId = editingId === provider.id ? '' : provider.id">{{ editingId === provider.id ? '收起' : '编辑' }}</button><button class="text-action muted-action" :disabled="saving" @click="remove(provider.id)">移除</button></div></div>
          <div v-if="editingId === provider.id" class="provider-fields">
            <label>名称<input :disabled="saving" v-model="provider.name" type="text" placeholder="为这项 AI 起个名字" /></label>
            <template v-if="provider.kind === 'http'"><label>接口地址<input :disabled="saving" v-model="provider.baseUrl" type="url" placeholder="https://example.com/v1" spellcheck="false" /></label><label>API Key<input :disabled="saving" v-model="provider.apiKey" type="password" placeholder="填写 API Key，本地服务可留空" autocomplete="off" spellcheck="false" /></label><ModelPicker v-model="provider.model" :base-url="provider.baseUrl" :api-key="provider.apiKey" :disabled="saving" /><p class="field-hint">支持 Chat Completions 兼容接口，密钥保存在本机应用配置中。</p></template>
            <template v-else><label>CLI 路径<input :disabled="saving" v-model="provider.executable" type="text" spellcheck="false" /></label><label>模型（可选）<input :disabled="saving" v-model="provider.model" type="text" placeholder="留空使用 CLI 当前默认模型" spellcheck="false" /></label><p class="field-hint">使用 CLI 已有的登录状态。首次使用前请在终端完成登录。</p></template>
          </div>
        </article>
      </div>
      <div class="default-ai-control" v-if="draft.providers.length"><button class="text-action muted-action" :disabled="saving || !draft.defaultProviderId" @click="selectDefault(null)">暂不设置默认 AI</button><span v-if="!draft.defaultProviderId">尚未选择默认 AI</span></div><div class="ai-settings-footer"><span>只有点击“AI 润色”时，才会将当前日报发送至默认 AI。</span><button class="text-action" :disabled="saving" @click="flush">{{ saving ? '正在保存…' : '保存 AI 设置' }}</button></div>
      <p v-if="error || localError" class="settings-error" role="alert">{{ error || localError }}</p><p v-else-if="saving" class="settings-muted" role="status">正在保存 AI 设置…</p><p v-else-if="!unchanged" class="settings-muted" role="status">设置尚未保存，返回首页时会自动保存。</p><p v-else-if="saved" class="settings-success" role="status">AI 设置已保存。</p>
    </section>
  </main>
</template>
