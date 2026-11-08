<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import {
  PhArchive,
  PhArrowClockwise,
  PhCheckCircle,
  PhDownloadSimple,
  PhUploadSimple,
  PhWarning,
  PhX,
} from '@phosphor-icons/vue'
import { createDialogMotion } from '../lib/dialogMotion'
import {
  createBackupBundle,
  downloadBackupFile,
  executeRestore,
  parseBackupJson,
  planRestore,
  type RestorePlan,
  type RestoreResult,
  type WorkspaceBackupBundle,
} from '../lib/backupMigration'
import type { Document, Entry, FileFormat } from '../lib/workspace'
import type { WorkspaceSettings } from '../lib/settingsDraft'
import { pushMessage } from '../lib/messages'

const props = defineProps<{
  visible: boolean
  workspaceName: string
  workspacePath?: string
  entries: Entry[]
  readEntry: (date: string, format: FileFormat) => Promise<Document>
  createEntry: (date: string, format: FileFormat, content: string) => Promise<Document>
  saveEntry: (date: string, content: string, expectedRevision: string, format: FileFormat) => Promise<Document>
  getSettings?: () => Promise<WorkspaceSettings | null>
  saveSettings?: (settings: WorkspaceSettings) => Promise<unknown>
}>()

const emit = defineEmits<{
  close: []
  restored: [result: RestoreResult]
}>()

const dialog = ref<HTMLDialogElement>()
const motion = createDialogMotion(() => dialog.value)

type ActiveTab = 'export' | 'restore'
const activeTab = ref<ActiveTab>('export')

// Export state
const exporting = ref(false)
const exportDone = ref(false)
const exportedCount = ref(0)

// Restore state
const fileInput = ref<HTMLInputElement>()
const restoreBundle = ref<WorkspaceBackupBundle | null>(null)
const restorePlan = ref<RestorePlan | null>(null)
const restorePolicy = ref<'skip' | 'overwrite'>('skip')
const restoreSettings = ref(true)
const parseError = ref('')
const restoring = ref(false)
const restoreResult = ref<RestoreResult | null>(null)

watch(
  () => props.visible,
  (val) => {
    if (val) {
      exporting.value = false
      exportDone.value = false
      parseError.value = ''
      restoreBundle.value = null
      restorePlan.value = null
      restoreResult.value = null
      motion.open()
      void nextTick(() => {
        dialog.value?.focus()
      })
    } else {
      void motion.close()
    }
  },
  { immediate: true, flush: 'post' },
)

onBeforeUnmount(() => {
  motion.dispose()
})

async function handleExport() {
  if (exporting.value) return
  exporting.value = true
  exportDone.value = false

  try {
    const bundle = await createBackupBundle({
      workspaceName: props.workspaceName,
      workspacePath: props.workspacePath,
      includeSettings: true,
      entries: props.entries,
      readEntry: props.readEntry,
      getSettings: props.getSettings,
    })

    downloadBackupFile(bundle)
    exportedCount.value = bundle.totalJournals
    exportDone.value = true
    pushMessage({ tone: 'success', text: `成功导出 ${bundle.totalJournals} 篇日报完整备份包！` })
  } catch (err) {
    pushMessage({ tone: 'error', text: `导出备份失败: ${err instanceof Error ? err.message : String(err)}` })
  } finally {
    exporting.value = false
  }
}

function triggerFileSelect() {
  fileInput.value?.click()
}

function onFileSelected(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return

  parseError.value = ''
  restoreResult.value = null

  const reader = new FileReader()
  reader.onload = () => {
    try {
      const content = String(reader.result || '')
      const bundle = parseBackupJson(content)
      restoreBundle.value = bundle
      restorePlan.value = planRestore(bundle, props.entries)
      restoreSettings.value = !!bundle.settings
    } catch (err) {
      parseError.value = err instanceof Error ? err.message : String(err)
      restoreBundle.value = null
      restorePlan.value = null
    }
  }
  reader.onerror = () => {
    parseError.value = '读取本地备份文件失败'
  }
  reader.readAsText(file)
  target.value = ''
}

async function handleRestore() {
  if (!restoreBundle.value || restoring.value) return
  restoring.value = true

  try {
    const result = await executeRestore({
      bundle: restoreBundle.value,
      policy: restorePolicy.value,
      restoreSettings: restoreSettings.value,
      existingEntries: props.entries,
      createEntry: props.createEntry,
      saveEntry: props.saveEntry,
      readEntry: props.readEntry,
      saveSettings: props.saveSettings,
    })

    restoreResult.value = result
    pushMessage({
      tone: 'success',
      text: `已完成恢复！共导入 ${result.totalRestored} 篇日报（新增 ${result.createdCount} 篇，覆盖 ${result.overwrittenCount} 篇）。`,
    })
    emit('restored', result)
  } catch (err) {
    pushMessage({ tone: 'error', text: `恢复数据失败: ${err instanceof Error ? err.message : String(err)}` })
  } finally {
    restoring.value = false
  }
}

function formatExportedAt(dateStr: string): string {
  try {
    const d = new Date(dateStr)
    return isNaN(d.getTime()) ? dateStr : d.toLocaleString()
  } catch {
    return dateStr
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    if (restoring.value) return
    event.preventDefault()
    emit('close')
  }
}
</script>

<template>
  <Teleport to="body">
    <dialog
      id="backup-restore-dialog"
      ref="dialog"
      tabindex="-1"
      role="dialog"
      aria-modal="true"
      class="backup-dialog confirmation-dialog"
      aria-labelledby="backup-dialog-title"
      @cancel.prevent="!restoring && emit('close')"
      @click.self="!restoring && emit('close')"
      @keydown="onKeydown"
    >
      <div class="backup-window">
        <!-- 弹窗顶栏 -->
        <header class="backup-header">
          <div class="backup-header-title">
            <div class="backup-title-icon" aria-hidden="true">
              <PhArchive :size="20" />
            </div>
            <div>
              <h3 id="backup-dialog-title" class="backup-title">工作空间备份与数据迁移</h3>
              <p class="backup-subtitle">跨设备一键迁移、数据归档与完整灾备恢复</p>
            </div>
          </div>
          <button
            type="button"
            class="backup-close-btn"
            aria-label="关闭弹窗 (Esc)"
            title="关闭 (Esc)"
            @click="emit('close')"
          >
            <PhX :size="16" aria-hidden="true" />
          </button>
        </header>

        <!-- 选项卡导航 -->
        <div class="backup-tabs" role="tablist" aria-label="备份迁移操作类型">
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'export'"
            class="backup-tab-btn"
            :class="{ 'is-active': activeTab === 'export' }"
            @click="activeTab = 'export'"
          >
            <PhDownloadSimple :size="15" aria-hidden="true" />
            <span>导出完整备份</span>
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'restore'"
            class="backup-tab-btn"
            :class="{ 'is-active': activeTab === 'restore' }"
            @click="activeTab = 'restore'"
          >
            <PhUploadSimple :size="15" aria-hidden="true" />
            <span>从备份包恢复</span>
          </button>
        </div>

        <!-- 弹窗主体区 -->
        <main class="backup-body">
          <!-- 导出选项卡内容 -->
          <div v-if="activeTab === 'export'" class="backup-tab-content">
            <div class="backup-info-card">
              <div class="info-row">
                <span class="info-label">当前工作空间：</span>
                <span class="info-value"
                  ><strong>{{ workspaceName }}</strong></span
                >
              </div>
              <div class="info-row">
                <span class="info-label">待备份日报数量：</span>
                <span class="info-value"
                  >共 <strong>{{ entries.length }}</strong> 篇</span
                >
              </div>
              <div class="info-row">
                <span class="info-label">备份包含数据：</span>
                <span class="info-value">所有历史日报、预设模板、AI 提示词、Git 仓库配置与界面偏好</span>
              </div>
            </div>

            <p class="backup-instruction">
              生成的备份文件为标准的 <code>.json</code> 结构化格式，可用于本地归档冷备，或直接导入到任意运行 WorkReport
              的设备中快速还原完整工作空间。
            </p>

            <div v-if="exportDone" class="export-success-banner" role="status">
              <PhCheckCircle :size="18" class="success-icon" aria-hidden="true" />
              <span>已成功打包并下载包含 {{ exportedCount }} 篇日报的备份文件！</span>
            </div>

            <div class="backup-action-area">
              <button
                type="button"
                class="dialog-button dialog-button-primary backup-primary-btn"
                :disabled="exporting"
                @click="handleExport"
              >
                <PhArrowClockwise v-if="exporting" class="spin" :size="15" aria-hidden="true" />
                <PhDownloadSimple v-else :size="15" aria-hidden="true" />
                <span>{{ exporting ? '正在读取并打包日报…' : '立即生成并下载备份文件' }}</span>
              </button>
            </div>
          </div>

          <!-- 恢复选项卡内容 -->
          <div v-else class="backup-tab-content">
            <input
              ref="fileInput"
              type="file"
              accept=".json"
              class="hidden-file-input"
              aria-label="选择备份文件"
              @change="onFileSelected"
            />

            <!-- 文件上传选择区 -->
            <div
              v-if="!restoreBundle"
              class="backup-dropzone"
              tabindex="0"
              role="button"
              aria-label="选择本地备份文件"
              @click="triggerFileSelect"
              @keydown.enter.prevent="triggerFileSelect"
              @keydown.space.prevent="triggerFileSelect"
            >
              <PhArchive :size="32" class="dropzone-icon" aria-hidden="true" />
              <div class="dropzone-copy">
                <strong>点击选择本地备份文件 (.json)</strong>
                <p>支持导入由 WorkReport 导出的全量备份文件</p>
              </div>
            </div>

            <!-- 解析错误提示 -->
            <div v-if="parseError" class="backup-error-banner" role="alert">
              <PhWarning :size="16" aria-hidden="true" />
              <span>{{ parseError }}</span>
            </div>

            <!-- 备份详情与恢复预览 -->
            <div v-if="restoreBundle && restorePlan" class="restore-preview-wrap">
              <div class="backup-info-card">
                <div class="info-row">
                  <span class="info-label">来源工作空间：</span>
                  <span class="info-value"
                    ><strong>{{ restoreBundle.workspaceName }}</strong></span
                  >
                </div>
                <div class="info-row">
                  <span class="info-label">备份生成时间：</span>
                  <span class="info-value">{{ formatExportedAt(restoreBundle.exportedAt) }}</span>
                </div>
                <div class="info-row">
                  <span class="info-label">备份日报总数：</span>
                  <span class="info-value"
                    >共 <strong>{{ restorePlan.totalInBackup }}</strong> 篇</span
                  >
                </div>
                <div class="info-row">
                  <span class="info-label">比对本地现状：</span>
                  <span class="info-value">
                    <span class="stat-highlight new">{{ restorePlan.newEntriesCount }} 篇新增</span>，
                    <span class="stat-highlight conflict">{{ restorePlan.conflictingEntriesCount }} 篇与本地同名</span>
                  </span>
                </div>
              </div>

              <!-- 冲突处理策略 -->
              <div class="restore-options-card">
                <h4 class="options-title">同名日报处理策略</h4>
                <div class="radio-group" role="radiogroup">
                  <label class="radio-label">
                    <input v-model="restorePolicy" type="radio" value="skip" name="restore-policy" />
                    <div>
                      <strong>仅恢复缺失日期（推荐）</strong>
                      <p>跳过与本地日期相同的日报，保护本地当前编辑内容不被覆盖。</p>
                    </div>
                  </label>
                  <label class="radio-label">
                    <input v-model="restorePolicy" type="radio" value="overwrite" name="restore-policy" />
                    <div>
                      <strong>完全覆盖本地已有同名日报</strong>
                      <p>将冲突日期的内容更新为备份文件中的内容。</p>
                    </div>
                  </label>
                </div>

                <div v-if="restorePlan.hasSettings" class="settings-checkbox-wrap">
                  <label class="checkbox-label">
                    <input v-model="restoreSettings" type="checkbox" />
                    <span>同时恢复工作空间配置（日报模板、AI 提示词与偏好）</span>
                  </label>
                </div>
              </div>

              <!-- 恢复结果反馈 -->
              <div v-if="restoreResult" class="export-success-banner" role="status">
                <PhCheckCircle :size="18" class="success-icon" aria-hidden="true" />
                <span>
                  数据恢复完成！成功导入 {{ restoreResult.totalRestored }} 篇（新增
                  {{ restoreResult.createdCount }} 篇，覆盖 {{ restoreResult.overwrittenCount }} 篇，跳过
                  {{ restoreResult.skippedCount }} 篇）。
                </span>
              </div>

              <!-- 操作按钮 -->
              <div class="restore-action-bar">
                <button
                  type="button"
                  class="dialog-button dialog-button-secondary"
                  :disabled="restoring"
                  @click="restoreBundle = null"
                >
                  重新选择文件
                </button>
                <button
                  type="button"
                  class="dialog-button dialog-button-primary backup-primary-btn"
                  :disabled="restoring"
                  @click="handleRestore"
                >
                  <PhArrowClockwise v-if="restoring" class="spin" :size="15" aria-hidden="true" />
                  <PhUploadSimple v-else :size="15" aria-hidden="true" />
                  <span>{{ restoring ? '正在恢复写入数据…' : '确认恢复数据' }}</span>
                </button>
              </div>
            </div>
          </div>
        </main>

        <!-- 弹窗底栏 -->
        <footer class="backup-footer">
          <div class="backup-footer-left">
            <span>备份与恢复全程在本地执行，绝不上传到任何外部第三方服务器</span>
          </div>
          <button type="button" class="dialog-button dialog-button-secondary" @click="emit('close')">
            完成并关闭 (Esc)
          </button>
        </footer>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.backup-dialog {
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  max-width: min(640px, calc(100vw - 32px));
  width: min(640px, calc(100vw - 32px));
}

.backup-window {
  display: flex;
  flex-direction: column;
  background: var(--skin-surface);
  border: 1px solid var(--skin-border);
  border-radius: 12px;
  box-shadow: var(--skin-shadow-raised);
  overflow: hidden;
}

.backup-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px 14px;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-surface-raised, var(--skin-surface));
}

.backup-header-title {
  display: flex;
  align-items: center;
  gap: 12px;
}

.backup-title-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 9px;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}

.backup-title {
  margin: 0;
  font-size: var(--text-title);
  font-weight: 600;
  color: var(--skin-text);
}

.backup-subtitle {
  margin: 2px 0 0;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
}

.backup-close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--skin-text-muted);
  cursor: pointer;
  transition: all 140ms ease;
}

.backup-close-btn:hover {
  background: var(--skin-canvas);
  color: var(--skin-text);
}

.backup-tabs {
  display: flex;
  border-bottom: 1px solid var(--skin-border);
  background: var(--skin-canvas);
  padding: 0 16px;
  gap: 8px;
}

.backup-tab-btn {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 40px;
  padding: 0 14px;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--skin-text-muted);
  font-size: var(--text-body);
  font-weight: 500;
  cursor: pointer;
  transition: all 140ms ease;
}

.backup-tab-btn:hover {
  color: var(--skin-text);
}

.backup-tab-btn.is-active {
  border-bottom-color: var(--skin-accent);
  color: var(--skin-accent);
}

.backup-body {
  padding: 20px;
  max-height: 480px;
  overflow-y: auto;
}

.backup-info-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 16px;
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  background: var(--skin-canvas);
  margin-bottom: 16px;
}

.info-row {
  display: flex;
  align-items: baseline;
  font-size: var(--text-body);
  line-height: 1.5;
}

.info-label {
  color: var(--skin-text-muted);
  width: 120px;
  flex-shrink: 0;
}

.info-value {
  color: var(--skin-text);
  word-break: break-all;
}

.stat-highlight.new {
  color: var(--skin-success);
  font-weight: 500;
}

.stat-highlight.conflict {
  color: var(--skin-warning, #d97706);
  font-weight: 500;
}

.backup-instruction {
  font-size: var(--text-small);
  line-height: 1.6;
  color: var(--skin-text-muted);
  margin: 0 0 18px;
}

.backup-instruction code {
  padding: 1px 4px;
  border-radius: 4px;
  background: var(--skin-subtle);
  font-family: monospace;
}

.export-success-banner {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border: 1px solid var(--skin-success-soft);
  border-radius: 8px;
  background: var(--skin-success-soft);
  color: var(--skin-success);
  font-size: var(--text-body);
  margin-bottom: 18px;
}

.backup-action-area {
  display: flex;
  justify-content: flex-end;
}

.backup-primary-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 18px;
}

.hidden-file-input {
  display: none;
}

.backup-dropzone {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 36px 20px;
  border: 2px dashed var(--skin-border);
  border-radius: 10px;
  background: var(--skin-canvas);
  cursor: pointer;
  transition: all 140ms ease;
  text-align: center;
}

.backup-dropzone:hover {
  border-color: var(--skin-accent);
  background: var(--skin-accent-soft);
}

.dropzone-icon {
  color: var(--skin-accent);
}

.dropzone-copy strong {
  display: block;
  font-size: var(--text-subtitle);
  color: var(--skin-text);
  margin-bottom: 4px;
}

.dropzone-copy p {
  margin: 0;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
}

.backup-error-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border: 1px solid var(--skin-danger-border, #fca5a5);
  border-radius: 8px;
  background: var(--skin-danger-soft, #fef2f2);
  color: var(--skin-danger, #dc2626);
  font-size: var(--text-body);
  margin-top: 14px;
}

.restore-preview-wrap {
  display: flex;
  flex-direction: column;
}

.restore-options-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px 16px;
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  background: var(--skin-canvas);
  margin-bottom: 16px;
}

.options-title {
  margin: 0;
  font-size: var(--text-body);
  font-weight: 600;
  color: var(--skin-text);
}

.radio-group {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.radio-label {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  cursor: pointer;
  font-size: var(--text-body);
}

.radio-label input[type='radio'] {
  margin-top: 3px;
}

.radio-label strong {
  display: block;
  color: var(--skin-text);
  margin-bottom: 2px;
}

.radio-label p {
  margin: 0;
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
  line-height: 1.4;
}

.settings-checkbox-wrap {
  padding-top: 10px;
  border-top: 1px solid var(--skin-border);
}

.checkbox-label {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  font-size: var(--text-small);
  color: var(--skin-text);
}

.restore-action-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.backup-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 20px;
  border-top: 1px solid var(--skin-border);
  background: var(--skin-surface-raised, var(--skin-surface));
}

.backup-footer-left {
  font-size: var(--text-caption);
  color: var(--skin-text-muted);
}
</style>
