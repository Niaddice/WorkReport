<script setup lang="ts">
import { PhArrowRight, PhFileText, PhFolder, PhFolderOpen } from '@phosphor-icons/vue'

defineProps<{ picking: boolean; busy: boolean; recent: string | null; recentName: string | undefined; error: string }>()
const emit = defineEmits<{ choose: []; open: [path: string] }>()
</script>

<template>
  <main class="workspace-gate">
    <div class="welcome-art" aria-hidden="true">
      <div class="art-folder"><PhFolderOpen :size="68" weight="light" /></div>
      <span class="art-file"><PhFileText :size="30" weight="light" /></span>
    </div>
    <span class="welcome-label">你的工作，从这里开始</span>
    <h1>为日报，选一个家。</h1>
    <p>选择一个文件夹作为工作空间。<br />每天的记录，保存为 Markdown 或 TXT 文件。</p>
    <md-filled-button :disabled="picking || busy" @click="emit('choose')">
      <PhFolderOpen slot="icon" :size="19" />{{ picking || busy ? '正在打开…' : '选择工作空间' }}
    </md-filled-button>
    <button
      v-if="recent"
      class="recent-workspace"
      :disabled="picking || busy"
      :title="recent"
      @click="emit('open', recent)"
    >
      <PhFolder :size="18" />
      <span
        ><small>继续使用上次的工作空间</small><strong>{{ recentName }}</strong></span
      >
      <PhArrowRight :size="18" />
    </button>
    <p v-if="error" class="gate-error" role="alert">{{ error }}</p>
    <div class="welcome-footnote">文件保存在本地，随时可以用其他编辑器打开。</div>
  </main>
</template>
