export type SkinId = 'default'

export interface Skin {
  id: SkinId
  name: string
  description: string
  /** 画布、卡片、强调色，仅用于弹窗里的缩略预览。 */
  swatches: [string, string, string]
}

/** 皮肤注册表；后续新增皮肤时在这里追加，并在 style.css 中补 `[data-skin='<id>']`。 */
export const SKINS: readonly Skin[] = [
  {
    id: 'default',
    name: '默认',
    description: '浅色留白与蓝灰强调，当前版本唯一的皮肤。',
    swatches: ['#f8fafd', '#ffffff', '#0b57d0'],
  },
]

export const DEFAULT_SKIN: SkinId = 'default'

export function skinById(id: string): Skin {
  return SKINS.find((skin) => skin.id === id) ?? SKINS[0]!
}
