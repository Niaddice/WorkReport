import type { Component } from 'vue'
import type { SkinId } from '../lib/skins'

/** 内置模块 id；新增模块只需在 registry 中登记，并在持久化层保持字符串兼容。 */
export type ModuleId = 'calendar' | 'editor' | 'overview' | 'history' | 'summary' | 'excel' | 'directory'

export interface ModulePlacement {
  id: ModuleId
}

/** 模块目录条目：界面、拖放与模块目录弹窗都从这里读取元数据。 */
export interface ModuleDefinition {
  id: ModuleId
  title: string
  description: string
  icon: Component
  component: Component
  /** 可选：渲染在模块标题栏（标题之后）的动态扩展内容。 */
  titleExtra?: Component
  /** 模块使用说明，点击标题栏帮助按钮时展示。 */
  help: readonly string[]
  /** 新工作空间的默认布局是否启用。 */
  defaultEnabled: boolean
}

/** 分栏方向：row = 左右分栏，column = 上下分栏。 */
export type SplitDirection = 'row' | 'column'

/** 拖放时相对目标模块的插入位置。 */
export type DropSide = 'left' | 'right' | 'top' | 'bottom'

export interface Rect {
  left: number
  top: number
  width: number
  height: number
}

export interface LayoutLeaf {
  type: 'leaf'
  id: ModuleId
}

export interface LayoutSplit {
  type: 'split'
  direction: SplitDirection
  /** first 占可用空间的比例，范围由 layout.ts 统一夹紧。 */
  ratio: number
  first: LayoutNode
  second: LayoutNode
}

/** 可嵌套的分栏布局树：叶子是模块，split 是左右/上下分栏。 */
export type LayoutNode = LayoutLeaf | LayoutSplit

/** 布局树渲染后的模块矩形（相对模块板内边距）。 */
export interface LayoutSlot extends Rect {
  id: ModuleId
}

/** 分栏间隙（拖动手柄）的几何信息。 */
export interface LayoutGutter extends Rect {
  /** 从根节点到该 split 的 0/1 路径。 */
  path: number[]
  direction: SplitDirection
  ratio: number
  /** split 所在父矩形的尺寸，用于把指针位置换算成新的 ratio。 */
  parent: Rect
}

export interface LayoutGeometry {
  slots: LayoutSlot[]
  gutters: LayoutGutter[]
}

/** 用户保存的布局快照：名字 + 布局树。 */
export interface SavedLayout {
  name: string
  layout: LayoutNode
}

/** 与 Rust 侧 ui::Preferences 对应的工作空间界面偏好。 */
export interface UiPreferences {
  /** 模块板的分栏布局树。 */
  layout: LayoutNode
  skin: SkinId
  /** 用户保存的布局快照，随界面偏好一起持久化。 */
  savedLayouts: SavedLayout[]
}

export const MAX_MODULES = 12

/** 稳定顺序，用于模块目录与旧布局迁移校验。 */
export const MODULE_IDS: readonly ModuleId[] = [
  'calendar',
  'editor',
  'overview',
  'history',
  'summary',
  'excel',
  'directory',
]

export function isModuleId(value: unknown): value is ModuleId {
  return typeof value === 'string' && (MODULE_IDS as readonly string[]).includes(value)
}
