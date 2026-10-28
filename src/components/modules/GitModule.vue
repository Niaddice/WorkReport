<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhCopy, PhGearSix, PhGitCommit, PhNotePencil } from '@phosphor-icons/vue'
import { useModuleContext } from '../../modules/context'
import { gitDailyReport } from '../../lib/useGit'
import { api } from '../../lib/workspace'
import { pushMessage } from '../../lib/messages'

const { git, journal, actions, disabled, today, rangeDates } = useModuleContext()
const { settings, reading, readError, result } = git
const report = ref('')
const applying = ref(false)
const localError = ref('')
const scope = computed(() => `${journal.workspace.value?.path ?? ''}/${journal.selectedDate.value}`)
const dateLabel = computed(() => dayjs(journal.selectedDate.value).format('YYYY年M月D日'))
const total = computed(
  () => result.value?.repositories.reduce((count, repository) => count + repository.commits.length, 0) ?? 0,
)
const failed = computed(() => result.value?.repositories.filter((repository) => repository.error).length ?? 0)
const unavailable = computed(() => disabled.value || git.loading.value || applying.value)
const singleDay = computed(() => rangeDates.value.length === 0)
const ready = computed(() => result.value?.date === journal.selectedDate.value && !!report.value.trim())
const alreadyIncluded = computed(() => !!report.value.trim() && journal.draft.value.includes(report.value.trim()))
watch(
  scope,
  () => {
    git.clear()
    report.value = ''
    localError.value = ''
  },
  { flush: 'sync' },
)
watch(result, (value) => {
  report.value = value ? gitDailyReport(value) : ''
  localError.value = ''
})
onBeforeUnmount(() => git.clear())

async function read() {
  const path = journal.workspace.value?.path
  if (!path || unavailable.value || reading.value || !singleDay.value) return
  localError.value = ''
  await git.read(path, journal.selectedDate.value)
}
async function copy() {
  try {
    await api.copy(report.value)
    pushMessage({ tone: 'success', text: 'Git 日报已复制' })
  } catch (cause) {
    localError.value = `复制失败：${String(cause)}`
  }
}
async function append() {
  const path = journal.workspace.value?.path
  if (!path || !ready.value || unavailable.value || !singleDay.value || reading.value) return
  const key = scope.value
  const content = report.value
  applying.value = true
  localError.value = ''
  try {
    if (
      !(await actions.appendGitReport(path, journal.selectedDate.value, content)) &&
      scope.value === key &&
      journal.error.value
    ) {
      localError.value = '追加内容已保留在编辑区，请检查日报保存状态。'
    }
  } catch (cause) {
    if (scope.value === key) localError.value = `追加日报失败：${String(cause)}`
  } finally {
    applying.value = false
  }
}
</script>

<template>
  <div class="git-module" :aria-busy="reading">
    <div class="git-toolbar">
      <div class="git-date">
        <strong>{{ dateLabel }}</strong
        ><span>本地提交 · {{ settings.repositories.length }} 个仓库</span>
      </div>
      <div class="git-toolbar-actions">
        <button
          type="button"
          class="text-action"
          :disabled="unavailable || (journal.selectedDate.value === today && singleDay)"
          @click="actions.selectReport(today)"
        >
          今天
        </button>
        <button
          type="button"
          class="icon-button"
          aria-label="Git 配置"
          title="Git 配置"
          :disabled="unavailable"
          @click="actions.openGitSettings"
        >
          <PhGearSix :size="18" />
        </button>
        <button
          type="button"
          class="git-fetch"
          :disabled="unavailable || reading || !settings.repositories.length || !singleDay"
          @click="read"
        >
          <PhArrowClockwise :size="16" />{{ reading ? '正在读取…' : '获取提交' }}
        </button>
      </div>
    </div>
    <div class="git-content">
      <div v-if="!settings.repositories.length" class="git-empty">
        <PhGitCommit :size="36" weight="light" aria-hidden="true" />
        <strong>从提交记录开始今天的日报</strong>
        <p>关联本地项目仓库，按项目整理当天的工作记录。</p>
        <button type="button" class="text-action" :disabled="unavailable" @click="actions.openGitSettings">
          添加 Git 仓库
        </button>
      </div>
      <template v-else>
        <p v-if="!singleDay" class="git-notice">Git 日报每次读取一天。请在日历中选择单日，或点击“今天”。</p>
        <p v-if="readError || localError || git.error.value" class="field-error" role="alert">
          {{ readError || localError || git.error.value }}
        </p>
        <p v-if="reading" class="git-hint" role="status">正在读取本地仓库的提交记录…</p>
        <p v-else-if="!result" class="git-hint">点击“获取提交”，查看 {{ dateLabel }} 的工作记录。默认忽略合并提交。</p>
        <template v-if="result">
          <p class="git-result-caption" role="status">
            {{ total }} 条提交<span v-if="failed"> · {{ failed }} 个仓库读取失败，可检查配置后重试</span>
          </p>
          <section v-for="repository in result.repositories" :key="repository.id" class="git-repository-log">
            <div class="git-repository-title">
              <strong>{{ repository.name }}</strong
              ><span>{{ repository.commits.length }} 条提交</span>
            </div>
            <p v-if="repository.error" class="field-error" role="alert">{{ repository.error }}</p>
            <p v-else-if="!repository.commits.length && !repository.duplicateCommits" class="git-hint">
              当天没有符合作者和分支筛选条件的提交。
            </p>
            <p v-if="repository.duplicateCommits" class="git-hint" role="status">
              已跳过 {{ repository.duplicateCommits }} 条与其他来源重复的提交。
            </p>
            <p v-if="repository.truncated" class="git-notice">
              仅显示前 1000 条提交，请在设置中缩小作者或分支范围后重新获取。
            </p>
            <ol v-if="repository.commits.length" class="git-commits">
              <li v-for="commit in repository.commits" :key="commit.hash">
                <details>
                  <summary>
                    <span>{{ commit.message.split('\n')[0] || '未填写提交说明' }}</span
                    ><code>{{ commit.hash.slice(0, 7) }}</code>
                  </summary>
                  <div class="git-commit-meta">
                    {{ commit.author }} · {{ dayjs(commit.committedAt).format('HH:mm:ss')
                    }}<span>{{ commit.email }}</span>
                  </div>
                  <pre>{{ commit.message }}</pre>
                </details>
              </li>
            </ol>
          </section>
          <section v-if="total" class="git-report-preview">
            <label for="git-report-content">日报预览<span>可在追加前修改</span></label>
            <textarea
              id="git-report-content"
              v-model="report"
              :disabled="applying || reading"
              aria-label="Git 日报预览"
              spellcheck="false"
            />
            <div class="git-report-actions">
              <button type="button" class="text-action" :disabled="!ready || applying || reading" @click="copy">
                <PhCopy :size="16" />复制
              </button>
              <button
                type="button"
                class="git-fetch"
                :disabled="!ready || unavailable || reading || !singleDay || alreadyIncluded || journal.saving.value"
                @click="append"
              >
                <PhNotePencil :size="16" />{{
                  applying ? '正在保存…' : alreadyIncluded ? '已在日报中' : '追加到当日日报'
                }}
              </button>
            </div>
            <p class="git-hint">追加到 {{ dateLabel }} 的编辑区，保留已有内容并自动保存，可在编辑区撤销。</p>
          </section>
        </template>
      </template>
    </div>
  </div>
</template>

<style scoped>
.git-module {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  color: #39485d;
}
.git-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 20px;
  border-bottom: 1px solid #eef2f7;
}
.git-date {
  display: grid;
  gap: 5px;
}
.git-date strong {
  font-size: var(--text-title);
  font-weight: 500;
}
.git-date > span,
.git-hint,
.git-result-caption {
  color: #8190a5;
  font-size: var(--text-small);
  line-height: 1.7;
}
.git-toolbar-actions,
.git-report-actions,
.git-fetch,
.text-action {
  display: flex;
  align-items: center;
  gap: 8px;
}
.git-fetch {
  border: 1px solid #d3e3fd;
  border-radius: 10px;
  padding: 9px 12px;
  background: #e7f0fe;
  color: var(--skin-accent);
  font: inherit;
  font-size: var(--text-body);
  cursor: pointer;
}
.git-fetch:hover:not(:disabled) {
  background: #d3e3fd;
}
.icon-button {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border: 0;
  border-radius: 50%;
  background: transparent;
  color: #8190a5;
  cursor: pointer;
}
.icon-button:hover:not(:disabled) {
  background: #e7f0fe;
  color: var(--skin-accent);
}
.icon-button:disabled {
  opacity: 0.5;
  cursor: default;
}
.git-fetch:disabled {
  opacity: 0.5;
  cursor: default;
}
.git-fetch:focus-visible,
.icon-button:focus-visible,
textarea:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
}
.git-content {
  min-height: 0;
  overflow: auto;
  padding: 18px 20px;
}
.git-empty {
  display: grid;
  justify-items: center;
  align-content: center;
  gap: 14px;
  min-height: 240px;
  text-align: center;
  color: #8190a5;
}
.git-empty strong {
  color: #4b607c;
  font-size: var(--text-title);
  font-weight: 500;
}
.git-empty p {
  margin: 0;
  font-size: var(--text-body);
  line-height: 1.7;
}
.git-notice {
  padding: 10px 12px;
  background: #fff7e5;
  border-radius: 10px;
  color: #8a6421;
  font-size: var(--text-small);
  line-height: 1.7;
}
.git-repository-log {
  margin: 18px 0 24px;
}
.git-repository-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
  font-size: var(--text-body);
}
.git-repository-title strong {
  font-weight: 500;
  overflow-wrap: anywhere;
}
.git-repository-title > span {
  color: #8190a5;
  font-size: var(--text-small);
}
.git-commits {
  margin: 0 0 0 5px;
  padding: 0 0 0 18px;
  border-left: 2px solid #d3e3fd;
  list-style: none;
}
.git-commits li {
  position: relative;
  padding: 0 0 14px;
}
.git-commits li::before {
  content: '';
  position: absolute;
  left: -23px;
  top: 6px;
  width: 8px;
  height: 8px;
  border: 2px solid var(--skin-accent);
  border-radius: 50%;
  background: var(--skin-surface);
}
.git-commits summary {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  cursor: pointer;
  font-size: var(--text-body);
  line-height: 1.6;
}
.git-commits summary span {
  overflow-wrap: anywhere;
}
.git-commits code {
  color: #8190a5;
  font-size: var(--text-small);
}
.git-commit-meta {
  display: grid;
  gap: 3px;
  margin: 8px 0;
  color: #8190a5;
  font-size: var(--text-small);
  overflow-wrap: anywhere;
}
.git-commits pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font: inherit;
  font-size: var(--text-body);
  line-height: 1.7;
  padding: 12px;
  background: #f8fafd;
  border-radius: 10px;
}
.git-report-preview {
  display: grid;
  gap: 12px;
  padding-top: 18px;
  border-top: 1px solid #eef2f7;
}
.git-report-preview label {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 6px;
  font-size: var(--text-body);
}
.git-report-preview label span {
  color: #8190a5;
  font-size: var(--text-small);
}
.git-report-preview textarea {
  box-sizing: border-box;
  width: 100%;
  min-height: 180px;
  padding: 14px;
  border: 1px solid #dce5f0;
  border-radius: 12px;
  background: #f8fafd;
  color: inherit;
  font: inherit;
  font-size: var(--text-body);
  line-height: 1.8;
  resize: vertical;
}
.git-report-actions {
  flex-wrap: wrap;
  justify-content: flex-end;
}
.git-report-preview .git-hint {
  margin: 0;
}
.field-error {
  overflow-wrap: anywhere;
}
</style>
