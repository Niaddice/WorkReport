export type SkinId =
  'default' | 'jade' | 'apricot' | 'wisteria' | 'midnight' | 'graphite' | 'vscode-dark-2026' | 'vscode-light-2026'

export interface Skin {
  id: SkinId
  name: string
  description: string
  mode: 'light' | 'dark'
  /** 画布、卡片、强调色，仅用于弹窗里的缩略预览。 */
  swatches: [string, string, string]
}

/** 皮肤注册表；配色变量集中在 src/skins.css，随工作空间保存。 */
export const SKINS: readonly Skin[] = [
  {
    id: 'default',
    name: '默认',
    description: '清透留白，经典蓝灰。',
    mode: 'light',
    swatches: ['#f8fafd', '#ffffff', '#0b57d0'],
  },
  {
    id: 'jade',
    name: '青竹',
    description: '浅青底色，竹绿点缀。',
    mode: 'light',
    swatches: ['#f2f8f5', '#fcfffd', '#21745b'],
  },
  {
    id: 'apricot',
    name: '暖杏',
    description: '温润纸色，琥珀暖意。',
    mode: 'light',
    swatches: ['#fbf6ee', '#fffdf8', '#956019'],
  },
  {
    id: 'wisteria',
    name: '紫藤',
    description: '淡紫雾色，柔和安静。',
    mode: 'light',
    swatches: ['#f6f4fb', '#fefcff', '#7353a6'],
  },
  {
    id: 'midnight',
    name: '午夜蓝',
    description: '深蓝夜色，明净星光。',
    mode: 'dark',
    swatches: ['#111827', '#1b2536', '#a6c8ff'],
  },
  {
    id: 'graphite',
    name: '石墨',
    description: '沉稳灰阶，薄荷微光。',
    mode: 'dark',
    swatches: ['#191d20', '#242a2e', '#92d4bd'],
  },
  {
    id: 'vscode-dark-2026',
    name: 'VS Code 黑色 2026',
    description: 'Dark 2026 · 黑灰底色。',
    mode: 'dark',
    swatches: ['#191a1b', '#121314', '#48a0c7'],
  },
  {
    id: 'vscode-light-2026',
    name: 'VS Code 白色 2026',
    description: 'Light 2026 · 纯白底色。',
    mode: 'light',
    swatches: ['#fafafd', '#ffffff', '#0069cc'],
  },
]

export const DEFAULT_SKIN: SkinId = 'default'

export function skinById(id: string): Skin {
  return SKINS.find((skin) => skin.id === id) ?? SKINS[0]!
}
