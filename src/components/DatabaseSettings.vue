<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { isTauri } from '@tauri-apps/api/core'
import { PhArrowsLeftRight, PhDatabase, PhPlugsConnected } from '@phosphor-icons/vue'
import {
  cloneDatabaseSettings,
  databaseApi,
  databaseError,
  validateDatabaseSettings,
  type DatabaseConflictPolicy,
  type DatabaseEngine,
  type DatabaseSettings,
  type DatabaseTransferAction,
  type DatabaseTransferDirection,
  type DatabaseTransferResult,
} from '../lib/database'
import AppSelect from './AppSelect.vue'

const props = defineProps<{
  modelValue: DatabaseSettings
  busy: boolean
  workspacePath: string
  unchanged: boolean
  beforeTransfer?: () => Promise<boolean>
}>()
const emit = defineEmits<{
  'update:modelValue': [settings: DatabaseSettings]
  busy: [value: boolean]
  transferred: [result: DatabaseTransferResult]
}>()
const draft = computed(() => props.modelValue)
const testing = ref(false)
const transferring = ref(false)
const localError = ref('')
const connectionMessage = ref('')
const transferMessage = ref('')
const direction = ref<DatabaseTransferDirection>('filesToDatabase')
const directionOptions = [
  { value: 'filesToDatabase', label: '日报文件 → 数据库' },
  { value: 'databaseToFiles', label: '数据库 → 日报文件' },
]
const conflictOptions = [
  { value: 'abort', label: '停止操作，保留目标' },
  { value: 'overwrite', label: '覆盖目标内容' },
]
const tlsOptions = [
  { value: 'preferred', label: '优先使用 TLS' },
  { value: 'required', label: '必须使用 TLS' },
  { value: 'verifyFull', label: 'TLS 并校验证书与主机' },
  { value: 'disabled', label: '不使用 TLS' },
]
const pendingAction = ref<DatabaseTransferAction | null>(null)
const conflictPolicy = ref<DatabaseConflictPolicy>('abort')
const confirmOverwrite = ref(false)
const formBusy = computed(() => props.busy || testing.value || transferring.value)
const transferDisabled = computed(
  () => formBusy.value || !props.workspacePath || !props.unchanged || !draft.value.workspaceId,
)
const defaultPort = computed(() => (draft.value.engine === 'postgres' ? 5432 : 3306))
const sqliteLocation = computed(
  () => `${props.workspacePath.replace(/[\\/]+$/, '')}/${draft.value.sqlitePath.trim().replace(/\\/g, '/')}`,
)
const destination = computed(() => (direction.value === 'filesToDatabase' ? '数据库' : '日报文件'))
const migrationDescription = computed(() =>
  direction.value === 'filesToDatabase'
    ? '将文件中的日报复制并核对到数据库，成功后仅使用数据库读写日报。原日报文件继续保留。'
    : '将数据库中的日报复制并核对到工作空间，成功后关闭数据库存储，使用日报文件读写。原数据库数据继续保留。',
)
let revision = 0
let expectedTransferSettings = ''

function cancelConfirmation() {
  pendingAction.value = null
  confirmOverwrite.value = false
}

watch(
  () => props.modelValue,
  () => {
    revision++
    localError.value = ''
    connectionMessage.value = ''
    cancelConfirmation()
    if (JSON.stringify(props.modelValue) !== expectedTransferSettings) transferMessage.value = ''
  },
  { deep: true },
)
watch(
  () => props.workspacePath,
  () => {
    revision++
    localError.value = ''
    connectionMessage.value = ''
    transferMessage.value = ''
    expectedTransferSettings = ''
    cancelConfirmation()
  },
)
watch([direction, conflictPolicy], () => {
  localError.value = ''
  transferMessage.value = ''
  cancelConfirmation()
})
watch(
  () => props.unchanged,
  (unchanged) => {
    if (!unchanged) cancelConfirmation()
  },
)
onBeforeUnmount(() => {
  revision++
  emit('busy', false)
})

function update(value: Partial<DatabaseSettings>) {
  if (formBusy.value) return
  expectedTransferSettings = ''
  transferMessage.value = ''
  emit('update:modelValue', cloneDatabaseSettings({ ...draft.value, ...value }))
}

function chooseEngine(engine: DatabaseEngine) {
  if (formBusy.value || engine === draft.value.engine) return
  const oldDefault = draft.value.engine === 'postgres' ? 5432 : 3306
  const port = draft.value.port === oldDefault ? 0 : draft.value.port
  update({ engine, port })
}

function validate(): boolean {
  localError.value = ''
  if (testing.value || transferring.value) {
    localError.value = '请等待数据库操作完成后保存。'
    return false
  }
  if (!props.workspacePath) {
    localError.value = '请先选择工作空间，再配置数据库存储。'
    return false
  }
  localError.value = validateDatabaseSettings(draft.value)
  return !localError.value
}

async function testConnection() {
  if (formBusy.value || (!draft.value.enabled && !draft.value.workspaceId)) return
  if (!props.workspacePath) {
    localError.value = '请先选择工作空间，再测试数据库连接。'
    return
  }
  localError.value = validateDatabaseSettings({ ...draft.value, enabled: true })
  if (localError.value) return
  if (!isTauri()) {
    localError.value = '请在桌面应用中测试数据库连接。'
    return
  }
  const snapshot = cloneDatabaseSettings(draft.value)
  const workspacePath = props.workspacePath
  const currentRevision = revision
  testing.value = true
  localError.value = ''
  connectionMessage.value = ''
  emit('busy', true)
  try {
    const result = await databaseApi.test(workspacePath, snapshot)
    if (revision !== currentRevision || props.workspacePath !== workspacePath) return
    connectionMessage.value = `${databaseError(result.message || '连接成功', snapshot)}${result.recordCount === undefined ? '' : `，当前工作空间有 ${result.recordCount} 篇日报。`}`
  } catch (cause) {
    if (revision === currentRevision && props.workspacePath === workspacePath)
      localError.value = `连接测试失败：${databaseError(cause, snapshot)}`
  } finally {
    testing.value = false
    emit('busy', false)
  }
}

function requestTransfer(action: DatabaseTransferAction) {
  if (transferDisabled.value) return
  if (action === 'migrate' || conflictPolicy.value === 'overwrite') {
    pendingAction.value = action
    confirmOverwrite.value = false
    localError.value = ''
    transferMessage.value = ''
  } else void transfer(action)
}

async function transfer(action: DatabaseTransferAction) {
  if (transferDisabled.value || (conflictPolicy.value === 'overwrite' && !confirmOverwrite.value)) return
  if (!isTauri()) {
    localError.value = '请在桌面应用中复制或迁移日报数据。'
    return
  }
  const snapshot = cloneDatabaseSettings(draft.value)
  const workspacePath = props.workspacePath
  const selectedDirection = direction.value
  const selectedPolicy = conflictPolicy.value
  const currentRevision = revision
  transferring.value = true
  localError.value = ''
  connectionMessage.value = ''
  transferMessage.value = ''
  emit('busy', true)
  try {
    if (props.beforeTransfer && !(await props.beforeTransfer())) {
      localError.value = '当前日报尚未保存，请检查保存状态后重试。'
      return
    }
    if (revision !== currentRevision || props.workspacePath !== workspacePath || !props.unchanged) return
    const result = await databaseApi.transfer(workspacePath, selectedDirection, action, selectedPolicy)
    if (revision !== currentRevision || props.workspacePath !== workspacePath) return
    cancelConfirmation()
    expectedTransferSettings = JSON.stringify(cloneDatabaseSettings(result.settings))
    emit('transferred', result)
    transferMessage.value = databaseError(
      result.message ||
        `已向${selectedDirection === 'filesToDatabase' ? '数据库' : '日报文件'}复制 ${result.copied} 篇日报，跳过 ${result.skipped} 篇相同内容。`,
      snapshot,
    )
  } catch (cause) {
    if (revision === currentRevision && props.workspacePath === workspacePath)
      localError.value = `${action === 'copy' ? '复制' : '迁移'}失败：${databaseError(cause, snapshot)}`
  } finally {
    transferring.value = false
    emit('busy', false)
  }
}

defineExpose({ validate })
</script>

<template>
  <div class="database-settings">
    <div class="database-enable-row">
      <div class="database-enable-copy">
        <h3 id="database-enabled-label">启用数据库存储</h3>
        <p id="database-enabled-description">
          为日报增加数据库备份，默认在当前工作空间创建 SQLite，并同时保留日报文件。
        </p>
      </div>
      <button
        type="button"
        class="excel-enable-switch"
        role="switch"
        :aria-checked="draft.enabled"
        aria-labelledby="database-enabled-label"
        aria-describedby="database-enabled-description"
        :disabled="formBusy || !workspacePath"
        @click="update({ enabled: !draft.enabled })"
      >
        <span aria-hidden="true"></span>
      </button>
    </div>
    <p v-if="!workspacePath" class="settings-muted">请先选择工作空间，再配置数据库存储。</p>
    <p v-else-if="!draft.enabled && draft.workspaceId" class="field-hint database-disabled-note">
      {{ unchanged ? '当前仅使用文件存储' : '保存后将仅使用文件存储' }}，原数据库保留，可复制或重新迁入。
    </p>
    <Transition name="settings-reveal">
      <div v-if="draft.enabled || !!draft.workspaceId" class="settings-reveal-shell">
        <div class="settings-reveal-inner">
          <div class="database-settings-fields database-fields">
            <div class="database-field">
              <span id="database-engine-label" class="database-field-label">数据库类型</span>
              <div class="database-engine-options" role="group" aria-labelledby="database-engine-label">
                <button
                  v-for="engine in ['sqlite', 'mysql', 'postgres'] as const"
                  :key="engine"
                  type="button"
                  :class="{ active: draft.engine === engine }"
                  :aria-pressed="draft.engine === engine"
                  :disabled="formBusy"
                  @click="chooseEngine(engine)"
                >
                  <PhDatabase :size="17" aria-hidden="true" />{{
                    engine === 'sqlite' ? 'SQLite' : engine === 'mysql' ? 'MySQL' : 'PostgreSQL'
                  }}
                </button>
              </div>
            </div>

            <Transition name="settings-panel" mode="out-in">
              <div v-if="draft.engine === 'sqlite'" key="sqlite" class="database-field">
                <label for="database-sqlite-path">SQLite 文件</label>
                <input
                  id="database-sqlite-path"
                  :value="draft.sqlitePath"
                  :disabled="formBusy"
                  type="text"
                  placeholder="workreport.sqlite"
                  spellcheck="false"
                  aria-describedby="database-sqlite-hint"
                  @input="update({ sqlitePath: ($event.target as HTMLInputElement).value })"
                />
                <p id="database-sqlite-hint" class="field-hint">
                  填写工作空间内的相对路径，支持 .sqlite、.sqlite3 或 .db；留空使用
                  workreport.sqlite。保存设置后自动创建数据库。
                </p>
                <div class="database-location">
                  <PhDatabase :size="17" aria-hidden="true" /><span>{{ sqliteLocation }}</span>
                </div>
              </div>
              <div v-else key="remote" class="database-fields-grid database-connection-grid">
                <div class="database-field">
                  <label for="database-host">主机</label
                  ><input
                    id="database-host"
                    :value="draft.host"
                    :disabled="formBusy"
                    type="text"
                    placeholder="127.0.0.1"
                    spellcheck="false"
                    autocomplete="off"
                    @input="update({ host: ($event.target as HTMLInputElement).value })"
                  />
                </div>
                <div class="database-field">
                  <label for="database-port">端口</label
                  ><input
                    id="database-port"
                    :value="draft.port"
                    :disabled="formBusy"
                    type="number"
                    min="0"
                    max="65535"
                    step="1"
                    :placeholder="String(defaultPort)"
                    aria-describedby="database-port-hint"
                    @input="update({ port: Number(($event.target as HTMLInputElement).value) })"
                  />
                  <p id="database-port-hint" class="field-hint">0 使用默认端口 {{ defaultPort }}。</p>
                </div>
                <div class="database-field">
                  <label for="database-name">数据库名称</label
                  ><input
                    id="database-name"
                    :value="draft.database"
                    :disabled="formBusy"
                    type="text"
                    placeholder="workreport"
                    spellcheck="false"
                    autocomplete="off"
                    @input="update({ database: ($event.target as HTMLInputElement).value })"
                  />
                </div>
                <div class="database-field">
                  <label for="database-user">用户名</label
                  ><input
                    id="database-user"
                    :value="draft.user"
                    :disabled="formBusy"
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    @input="update({ user: ($event.target as HTMLInputElement).value })"
                  />
                </div>
                <div class="database-field">
                  <label for="database-password">密码</label
                  ><input
                    id="database-password"
                    :value="draft.password"
                    :disabled="formBusy"
                    type="password"
                    placeholder="按数据库要求填写，可留空"
                    spellcheck="false"
                    autocomplete="new-password"
                    @input="update({ password: ($event.target as HTMLInputElement).value })"
                  />
                </div>
                <div class="database-field">
                  <label for="database-tls-mode">TLS 连接</label
                  ><AppSelect
                    id="database-tls-mode"
                    :model-value="draft.tlsMode"
                    :options="tlsOptions"
                    :disabled="formBusy"
                    @update:model-value="update({ tlsMode: $event as DatabaseSettings['tlsMode'] })"
                  />
                </div>
                <p class="field-hint database-remote-hint">
                  请先在服务器创建数据库，并为当前用户授予建表、读取和写入权限。密码随设置保存在本机工作空间配置中。
                </p>
              </div>
            </Transition>

            <div class="database-field">
              <span id="database-mode-label" class="database-field-label">日报存储方式</span>
              <div class="database-mode-options" role="radiogroup" aria-labelledby="database-mode-label">
                <label :class="{ selected: draft.mode === 'dual' }" class="database-mode-label"
                  ><input
                    type="radio"
                    name="database-storage-mode"
                    value="dual"
                    :checked="draft.mode === 'dual'"
                    :disabled="formBusy"
                    @change="update({ mode: 'dual' })"
                  /><span class="database-mode-copy"
                    ><strong>文件与数据库双存储</strong><span>继续保存日报文件，同时写入数据库。</span></span
                  ></label
                >
                <label :class="{ selected: draft.mode === 'database' }" class="database-mode-label"
                  ><input
                    type="radio"
                    name="database-storage-mode"
                    value="database"
                    :checked="draft.mode === 'database'"
                    :disabled="formBusy"
                    @change="update({ mode: 'database' })"
                  /><span class="database-mode-copy"
                    ><strong>仅数据库</strong><span>通过数据库读写日报，已有文件继续保留。</span></span
                  ></label
                >
              </div>
              <p class="field-hint">
                已有日报可在下方复制或迁移。数据范围为日报正文与格式，不包括工作空间配置、Excel 模板和导出文件。
              </p>
            </div>

            <div class="database-connection-actions database-actions">
              <button type="button" class="text-action" :disabled="formBusy || !workspacePath" @click="testConnection">
                <PhPlugsConnected :size="17" aria-hidden="true" /><Transition name="button-label" mode="out-in"
                  ><span :key="String(testing)">{{ testing ? '正在测试…' : '测试连接' }}</span></Transition
                ></button
              ><span class="field-hint">{{
                draft.engine === 'sqlite'
                  ? '仅测试连接；SQLite 文件不存在时会创建，不保存设置。'
                  : '使用当前输入测试，不保存设置。'
              }}</span>
            </div>

            <section class="database-transfer-section database-transfer" aria-labelledby="database-transfer-title">
              <header>
                <h3 id="database-transfer-title"><PhArrowsLeftRight :size="18" aria-hidden="true" />复制与迁移</h3>
                <p>复制日报数据；迁移会在核对成功后切换后续存储方式，保留源数据。</p>
              </header>
              <div class="database-fields-grid">
                <div class="database-transfer-direction database-field">
                  <label for="database-transfer-direction">数据方向</label
                  ><AppSelect
                    id="database-transfer-direction"
                    :model-value="direction"
                    :options="directionOptions"
                    :disabled="formBusy"
                    @update:model-value="direction = $event as DatabaseTransferDirection"
                  />
                </div>
                <div class="database-field">
                  <label for="database-conflict-policy">目标内容或格式不同时</label
                  ><AppSelect
                    id="database-conflict-policy"
                    :model-value="conflictPolicy"
                    :options="conflictOptions"
                    :disabled="formBusy"
                    @update:model-value="conflictPolicy = $event as DatabaseConflictPolicy"
                  />
                </div>
              </div>
              <div class="database-transfer-actions database-actions">
                <button
                  type="button"
                  class="text-action"
                  :disabled="transferDisabled || !!pendingAction"
                  @click="requestTransfer('copy')"
                >
                  {{ transferring ? '正在处理…' : `复制到${destination}` }}</button
                ><button
                  type="button"
                  class="text-action"
                  :disabled="transferDisabled || !!pendingAction"
                  :aria-expanded="pendingAction === 'migrate'"
                  aria-controls="database-migration-confirm"
                  @click="requestTransfer('migrate')"
                >
                  迁移到{{ destination }}
                </button>
              </div>
              <p v-if="!unchanged" class="field-hint database-transfer-note">
                请先点击底部“保存设置”，再复制或迁移。操作使用已保存的数据库配置。
              </p>
              <p v-else-if="!draft.workspaceId" class="field-hint database-transfer-note">
                请先启用并保存数据库设置，再复制或迁移日报数据。
              </p>
              <p v-else class="field-hint database-transfer-note">
                同日期、同格式且内容一致的日报会跳过。{{
                  conflictPolicy === 'abort'
                    ? '发现不同内容或格式时停止操作，不覆盖已有日报。'
                    : '同日期、不同内容或格式的目标日报将被覆盖，执行前需要确认，源数据继续保留。'
                }}
              </p>
              <Transition name="settings-reveal"
                ><div v-if="pendingAction" id="database-migration-confirm" class="database-migration-confirm">
                  <p>
                    {{
                      pendingAction === 'migrate'
                        ? migrationDescription
                        : `将日报复制到${destination}，保留当前存储方式。`
                    }}
                  </p>
                  <label v-if="conflictPolicy === 'overwrite'" class="database-overwrite-confirm"
                    ><input v-model="confirmOverwrite" type="checkbox" :disabled="formBusy" /><span
                      >确认覆盖目标中同日期、不同内容或格式的日报，保留源数据。</span
                    ></label
                  >
                  <div class="database-actions">
                    <button type="button" class="text-action" :disabled="formBusy" @click="cancelConfirmation">
                      取消</button
                    ><button
                      type="button"
                      class="text-action"
                      :disabled="transferDisabled || (conflictPolicy === 'overwrite' && !confirmOverwrite)"
                      @click="transfer(pendingAction)"
                    >
                      {{ transferring ? '正在处理…' : pendingAction === 'migrate' ? '确认迁移' : '确认复制' }}
                    </button>
                  </div>
                </div></Transition
              >
            </section>
          </div>
        </div>
      </div>
    </Transition>
    <Transition name="status-fade" mode="out-in">
      <p v-if="localError" key="error" class="database-status settings-error" role="alert">{{ localError }}</p>
      <p v-else-if="transferMessage" key="transfer" class="database-status database-status-success" role="status">
        {{ transferMessage }}
      </p>
      <p v-else-if="connectionMessage" key="connection" class="database-status database-status-success" role="status">
        {{ connectionMessage }}
      </p>
    </Transition>
  </div>
</template>
