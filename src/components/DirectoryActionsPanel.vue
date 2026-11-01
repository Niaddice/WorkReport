<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhArrowUUpLeft, PhCircleNotch, PhTrashSimple, PhX } from '@phosphor-icons/vue'
import type { FilePlan, TrashEntry } from '../lib/workspaceFiles'

const props = defineProps<{
  action: 'delete' | 'trash'
  path: string
  plan: FilePlan | null
  loading: boolean
  busy: boolean
  error: string
  entries: TrashEntry[]
  databaseMode: boolean
}>()
const emit = defineEmits<{ close: []; confirm: []; retry: []; restore: [entry: TrashEntry] }>()
const panel = ref<HTMLElement>()
const titleId = useId()
const name = computed(() => props.path.split('/').pop())
defineExpose({ focus: () => panel.value?.focus({ preventScroll: true }) })
</script>

<template>
  <section
    ref="panel"
    class="directory-actions-panel"
    :class="{ 'is-delete': action === 'delete' }"
    role="region"
    :aria-labelledby="titleId"
    :aria-busy="loading || busy"
    tabindex="-1"
    @pointerdown.stop
    @contextmenu.prevent.stop
    @keydown.esc.prevent.stop="!busy && emit('close')"
  >
    <header class="directory-actions-heading">
      <PhTrashSimple :size="16" aria-hidden="true" />
      <h3 :id="titleId">{{ action === 'delete' ? '移到回收站' : '工作空间回收站' }}</h3>
      <button
        type="button"
        class="text-action"
        aria-label="关闭文件操作"
        title="关闭（Esc）"
        :disabled="busy"
        @click="emit('close')"
      >
        <PhX :size="15" aria-hidden="true" />
      </button>
    </header>
    <div class="directory-actions-body">
      <template v-if="action === 'delete'">
        <p class="directory-action-target" :title="path">{{ name }}</p>
        <p v-if="loading" role="status">正在检查目录内容…</p>
        <p v-else-if="plan">
          {{ plan.fileCount }} 个文件、{{ plan.directoryCount }} 个文件夹<span v-if="plan.journalCount"
            >，其中 {{ plan.journalCount }} 篇日报</span
          >。
        </p>
        <p>移入当前工作空间回收站后可恢复。</p>
        <p v-if="databaseMode">数据库中的已有副本保留。</p>
      </template>
      <template v-else>
        <p v-if="loading" role="status">正在读取回收站…</p>
        <p v-else-if="!entries.length" class="directory-trash-empty">回收站为空。</p>
        <div v-for="entry in entries" :key="entry.id" class="directory-trash-entry">
          <div>
            <strong :title="entry.path">{{ entry.name }}</strong>
            <span :title="entry.path">{{ entry.path }}</span>
            <small>{{ dayjs.unix(entry.deletedAt).format('MM-DD HH:mm') }} · {{ entry.fileCount }} 个文件</small>
          </div>
          <button
            type="button"
            class="text-action"
            :disabled="busy"
            :aria-label="`恢复 ${entry.name}`"
            :title="`恢复 ${entry.name}`"
            @click="emit('restore', entry)"
          >
            <PhArrowUUpLeft :size="16" aria-hidden="true" />
          </button>
        </div>
      </template>
      <p v-if="error" class="directory-action-error" role="alert">{{ error }}</p>
    </div>
    <footer v-if="action === 'delete'" class="directory-actions-footer">
      <button type="button" :disabled="busy" @click="emit('close')">取消</button>
      <button v-if="error && !plan" type="button" :disabled="busy || loading" @click="emit('retry')">
        <PhArrowClockwise :size="13" aria-hidden="true" />重新检查
      </button>
      <button
        v-else
        type="button"
        class="directory-delete-confirm"
        :disabled="busy || loading || !plan"
        @click="emit('confirm')"
      >
        <PhCircleNotch v-if="busy" class="spin" :size="13" aria-hidden="true" />
        {{ busy ? '正在处理…' : '移到回收站' }}
      </button>
    </footer>
  </section>
</template>
