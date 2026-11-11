import type { Component } from 'vue'
import type { SkinId } from '../lib/skins'

/** 内置模块 id；新增模块只需在 registry 中登记，并在持久化层保持字符串兼容。 */
export type ModuleType = 'calendar' | 'editor' | 'overview' | 'history' | 'summary' | 'excel' | 'directory' | 'git'

/** 保留旧独立日报预览的 id 类型供迁移读取；当前布局统一使用模块类型 id。 */
export type ModuleId = ModuleType | `editor-${number}`

export interface ModulePlacement {
  id: ModuleId
}

/** 模块自己的设置面板：标题栏的设置入口据此显示，打开后只编辑该模块的设置。 */
export interface ModuleSettingsDefinition {
  /** 弹窗标题，例如「日历设置」。 */
  title: string
  /** 弹窗标题下方的说明。 */
  description: string
  /** 设置内容组件；通过 useModuleSettingsContext() 读取草稿与动作。 */
  component: Component
  /** 是否有需要保存的草稿；为 false 时（例如放假信息同步）弹窗只提供关闭。 */
  savable?: boolean
}

/** 模块目录条目：界面、拖放与模块目录弹窗都从这里读取元数据。 */
export interface ModuleDefinition {
  id: ModuleType
  title: string
  description: string
  icon: Component
  component: Component
  /** 可选：渲染在模块标题栏（标题之后）的动态扩展内容。 */
  titleExtra?: Component
  /** 可选：模块自己的设置面板；存在时标题栏显示设置按钮。 */
  settings?: ModuleSettingsDefinition
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

/** 没有打开模块的布局；只用在布局根节点。 */
export interface EmptyLayout {
  type: 'empty'
}
export type Layout = LayoutNode | EmptyLayout

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

/** 模块板的实际像素尺寸和间距，供预览与保存使用同一套局部缩放计算。 */
export interface LayoutMetrics {
  width: number
  height: number
  gap: number
}

/** 用户保存的布局快照：名字 + 布局树。 */
export interface SavedLayout {
  name: string
  layout: Layout
}

/** 与 Rust 侧 ui::Preferences 对应的工作空间界面偏好。 */
export interface UiPreferences {
  /** 模块板的分栏布局树。 */
  layout: Layout
  skin: SkinId
  /** 用户保存的布局快照，随界面偏好一起持久化。 */
  savedLayouts: SavedLayout[]
}

export const MAX_MODULES = 12

/** 稳定顺序，用于模块目录与旧布局迁移校验。 */
export const MODULE_IDS: readonly ModuleType[] = [
  'calendar',
  'editor',
  'overview',
  'history',
  'summary',
  'excel',
  'directory',
  'git',
]

export function isModuleId(value: unknown): value is ModuleId {
  return (
    typeof value === 'string' &&
    ((MODULE_IDS as readonly string[]).includes(value) || /^editor-[1-9]\d{0,8}$/.test(value))
  )
}

export function moduleType(id: ModuleId): ModuleType {
  return id.startsWith('editor-') ? 'editor' : (id as ModuleType)
}
