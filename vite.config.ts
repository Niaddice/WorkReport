import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

/**
 * 依赖按用途拆成独立分块：主包只留业务代码，供应商包可长期缓存，
 * 也避免单文件超过 500 kB 的构建告警。
 */
function manualChunks(id: string): string | undefined {
  if (!id.includes('node_modules')) return undefined
  if (id.includes('@material/web')) return 'vendor-material'
  if (id.includes('@phosphor-icons')) return 'vendor-icons'
  if (id.includes('marked') || id.includes('dompurify')) return 'vendor-markdown'
  if (id.includes('dayjs')) return 'vendor-dayjs'
  if (id.includes('@tauri-apps')) return 'vendor-tauri'
  if (id.includes('/vue/') || id.includes('@vue/')) return 'vendor-vue'
  return 'vendor'
}

export default defineConfig({
  plugins: [vue({ template: { compilerOptions: { isCustomElement: (tag) => tag.startsWith('md-') } } })],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: { target: 'es2022', rollupOptions: { output: { manualChunks } } },
  // 每个测试文件都要独立加载模块；并发过高时 worker 启动会互相拖慢，
  // 甚至触发 Vitest 90 秒的 worker 启动上限。
  test: { maxWorkers: 2 },
})
