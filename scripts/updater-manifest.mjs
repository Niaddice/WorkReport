import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'

const [directory, version, platform, architecture, repository, notesFile] = process.argv.slice(2)
if (!directory || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version) || !/^[\w.-]+\/[\w.-]+$/.test(repository))
  throw new Error('更新清单参数无效')
const arch = {
  arm64: 'aarch64',
  aarch64: 'aarch64',
  x86_64: 'x86_64',
  amd64: 'x86_64',
  x64: 'x86_64',
  i686: 'i686',
  ia32: 'i686',
  armv7l: 'armv7',
}[architecture]
const target = { macos: 'darwin', linux: 'linux', windows: 'windows' }[platform]
if (!arch || !target) throw new Error(`不支持的更新平台：${platform}/${architecture}`)
const files = fs.readdirSync(directory)
const extension =
  platform === 'macos'
    ? '.app.tar.gz'
    : platform === 'linux'
      ? '.AppImage'
      : files.some((file) => file.endsWith('.exe'))
        ? '.exe'
        : '.msi'
const candidates = files.filter((file) => file.endsWith(extension))
if (candidates.length !== 1) throw new Error(`应有且仅有一个 ${extension} 更新包，实际为 ${candidates.length} 个`)
const artifact = candidates[0]
const signature = fs.readFileSync(path.join(directory, `${artifact}.sig`), 'utf8').trim()
if (!signature) throw new Error('更新包签名为空，拒绝发布')
const notes = notesFile ? fs.readFileSync(notesFile, 'utf8') : `WorkReport ${version} 更新。`
const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    [`${target}-${arch}`]: {
      signature,
      url: `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(artifact)}`,
    },
  },
}
fs.writeFileSync(path.join(directory, 'latest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
