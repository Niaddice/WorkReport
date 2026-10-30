import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { error } from 'node:console'
import { fileURLToPath, URL } from 'node:url'
import { spawnSync } from 'node:child_process'

const root = fileURLToPath(new URL('../', import.meta.url))
const env = { ...process.env }
if (!env.TAURI_SIGNING_PRIVATE_KEY) {
  const key = path.join(root, '.updater/workreport.key')
  if (!fs.existsSync(key)) {
    error(
      '缺少更新签名私钥。请恢复 .updater/workreport.key，或设置 TAURI_SIGNING_PRIVATE_KEY。详见 README 的应用自动更新。',
    )
    process.exit(1)
  }
  env.TAURI_SIGNING_PRIVATE_KEY = key
}
env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ??= ''
const result = spawnSync(
  process.execPath,
  [path.join(root, 'node_modules/@tauri-apps/cli/tauri.js'), 'build', ...process.argv.slice(2)],
  { cwd: root, env, stdio: 'inherit' },
)
if (result.error) error(result.error.message)
process.exit(result.status ?? 1)
