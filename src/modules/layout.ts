import { DEFAULT_SKIN, SKINS, type SkinId } from '../lib/skins'
import {
  MAX_MODULES,
  isModuleId,
  type DropSide,
  type LayoutGeometry,
  type LayoutGutter,
  type LayoutLeaf,
  type LayoutMetrics,
  type LayoutNode,
  type LayoutSlot,
  type LayoutSplit,
  type ModuleId,
  type ModulePlacement,
  type Rect,
  type SavedLayout,
  type SplitDirection,
  type UiPreferences,
} from './types'

/**
 * 分栏比例的安全区间。
 *
 * 产品要求「每个模块最窄可缩到 App 宽度的 1%」：单边比例下限取 0.01，
 * 上限取 0.99。局部拖动（resizeLayoutLocally）按实际宽高限制贴边模块的
 * 最小尺寸，并固定非相邻模块；已经小于根尺寸 1% 的模块不再继续缩小。
 */
const MIN_SPLIT_RATIO = 0.01
const MAX_SPLIT_RATIO = 0.99
const DEFAULT_SPLIT_RATIO = 0.5
/** 归一化新布局时的递归深度上限；合法布局最多 MAX_MODULES 个叶子，远达不到该深度。 */
const MAX_LAYOUT_DEPTH = MAX_MODULES
/** 保存布局的数量上限与名称长度上限（按字符数），与 Rust 侧保持一致。 */
export const MAX_SAVED_LAYOUTS = 20
export const MAX_LAYOUT_NAME_CHARS = 50

/** 新工作空间默认左右分栏：日历在左、日报编辑在右。 */
export const DEFAULT_UI_PREFERENCES: UiPreferences = {
  layout: {
    type: 'split',
    direction: 'row',
    ratio: DEFAULT_SPLIT_RATIO,
    first: { type: 'leaf', id: 'calendar' },
    second: { type: 'leaf', id: 'editor' },
  },
  skin: DEFAULT_SKIN,
  savedLayouts: [],
}

/** 统一夹紧分栏比例，保证拖动、迁移与持久化后的比例始终可用。 */
export function clampSplitRatio(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SPLIT_RATIO
  return Math.min(MAX_SPLIT_RATIO, Math.max(MIN_SPLIT_RATIO, value))
}

function createLeaf(id: ModuleId): LayoutLeaf {
  return { type: 'leaf', id }
}

function createSplit(direction: SplitDirection, ratio: number, first: LayoutNode, second: LayoutNode): LayoutSplit {
  return { type: 'split', direction, ratio, first, second }
}

function cloneLayout(layout: LayoutNode): LayoutNode {
  if (layout.type === 'leaf') return createLeaf(layout.id)
  return createSplit(layout.direction, layout.ratio, cloneLayout(layout.first), cloneLayout(layout.second))
}

/** 深拷贝界面偏好；布局树与保存的布局快照逐层复制，调用方修改结果不会污染默认值或原对象。 */
export function cloneUiPreferences(value: UiPreferences): UiPreferences {
  return {
    layout: cloneLayout(value.layout),
    skin: value.skin,
    savedLayouts: value.savedLayouts.map((saved) => ({ name: saved.name, layout: cloneLayout(saved.layout) })),
  }
}

function normalizeSkin(value: unknown): SkinId {
  return SKINS.find((skin) => skin.id === value)?.id ?? DEFAULT_SKIN
}

function layoutHasModule(layout: LayoutNode, id: ModuleId): boolean {
  if (layout.type === 'leaf') return layout.id === id
  return layoutHasModule(layout.first, id) || layoutHasModule(layout.second, id)
}

function layoutLeafCount(layout: LayoutNode): number {
  if (layout.type === 'leaf') return 1
  return layoutLeafCount(layout.first) + layoutLeafCount(layout.second)
}

/**
 * 布局树归一化：递归校验新 schema 的布局树。
 *
 * - 只接受 type 为 leaf/split 的节点，叶子 id 必须已注册；
 * - 同一模块只保留视觉顺序上的第一次出现，总数不超过 MAX_MODULES；
 * - split 的 ratio 夹紧到安全区间，direction 非法时回退 row；
 * - 非法子树直接丢弃，只剩一个合法子节点时用该子节点替代 split。
 */
function normalizeLayoutNode(value: unknown, seen: Set<ModuleId>, depth: number): LayoutNode | null {
  if (depth > MAX_LAYOUT_DEPTH || !value || typeof value !== 'object') return null
  const candidate = value as Record<string, unknown>
  if (candidate.type === 'leaf') {
    if (!isModuleId(candidate.id) || seen.has(candidate.id) || seen.size >= MAX_MODULES) return null
    seen.add(candidate.id)
    return createLeaf(candidate.id)
  }
  if (candidate.type !== 'split') return null
  const first = normalizeLayoutNode(candidate.first, seen, depth + 1)
  const second = normalizeLayoutNode(candidate.second, seen, depth + 1)
  if (!first || !second) return first ?? second
  const direction: SplitDirection = candidate.direction === 'column' ? 'column' : 'row'
  const ratio = typeof candidate.ratio === 'number' ? clampSplitRatio(candidate.ratio) : DEFAULT_SPLIT_RATIO
  return createSplit(direction, ratio, first, second)
}

/** 兼容旧 schema：从 { id } 列表或更旧的 order 字符串列表中提取合法、去重后的模块 id。 */
function collectModuleIds(value: unknown): ModuleId[] {
  const ids: ModuleId[] = []
  const seen = new Set<ModuleId>()
  if (!Array.isArray(value)) return ids
  for (const item of value) {
    const candidate = item && typeof item === 'object' ? (item as { id?: unknown }).id : item
    if (!isModuleId(candidate) || seen.has(candidate)) continue
    seen.add(candidate)
    ids.push(candidate)
    if (ids.length >= MAX_MODULES) break
  }
  return ids
}

/** 旧 schema 的列数：columns 只接受 1；更旧的 axis=column 视为单列，其余按两列迁移。 */
function legacyColumns(raw: Record<string, unknown>): 1 | 2 {
  if (raw.columns === 1) return 1
  if (!('columns' in raw) && raw.axis === 'column') return 1
  return 2
}

/** 把一串节点用 column split 等分堆叠：每层按剩余节点数取 1/n，保持各节点占比一致。 */
function stackVertically(nodes: LayoutNode[]): LayoutNode {
  const [first, ...rest] = nodes
  if (!rest.length) return first
  return createSplit('column', 1 / nodes.length, first, stackVertically(rest))
}

/**
 * 旧 { columns, modules } / { axis, order } 布局迁移：
 * 两列时模块两两成行（每行 row ratio 0.5），行之间用 column split 等分；
 * 单列时所有模块垂直等分。
 */
function buildLegacyLayout(ids: ModuleId[], columns: 1 | 2): LayoutNode {
  const leaves = ids.map(createLeaf)
  if (columns === 1) return stackVertically(leaves)
  const rows: LayoutNode[] = []
  for (let index = 0; index < leaves.length; index += 2) {
    const first = leaves[index]
    const second: LayoutNode | undefined = leaves[index + 1]
    rows.push(second ? createSplit('row', DEFAULT_SPLIT_RATIO, first, second) : first)
  }
  return stackVertically(rows)
}

/**
 * 把磁盘或浏览器缓存里的任意值归一化成可用布局：
 * - 新格式 { layout, skin }：布局树递归校验后优先采用；
 * - 旧格式 { columns, modules, skin } 与更旧的 { axis, order, skin }：迁移成等分分栏树；
 * - 损坏、空值或没有任何合法模块时回退默认布局，不抛错。
 */
export function normalizeUiPreferences(value: unknown): UiPreferences {
  const raw = value && typeof value === 'object' ? (value as Record<string, unknown>) : {}
  const skin = normalizeSkin(raw.skin)
  const savedLayouts = normalizeSavedLayouts(raw.savedLayouts)
  const layout = normalizeLayoutNode(raw.layout, new Set(), 0)
  if (layout) return { layout, skin, savedLayouts }
  const modules = collectModuleIds(raw.modules)
  const legacyIds = modules.length ? modules : collectModuleIds(raw.order)
  if (!legacyIds.length) return { layout: cloneLayout(DEFAULT_UI_PREFERENCES.layout), skin, savedLayouts }
  return { layout: buildLegacyLayout(legacyIds, legacyColumns(raw)), skin, savedLayouts }
}

/** 归一化保存的布局快照：过滤空名、超长名、重复名与非法布局树。 */
function normalizeSavedLayouts(value: unknown): SavedLayout[] {
  if (!Array.isArray(value)) return []
  const result: SavedLayout[] = []
  const seen = new Set<string>()
  for (const item of value) {
    if (!item || typeof item !== 'object') continue
    const candidate = item as { name?: unknown; layout?: unknown }
    const name = typeof candidate.name === 'string' ? candidate.name.trim() : ''
    if (!name || [...name].length > MAX_LAYOUT_NAME_CHARS || seen.has(name)) continue
    const layout = normalizeLayoutNode(candidate.layout, new Set(), 0)
    if (!layout) continue
    seen.add(name)
    result.push({ name, layout })
    if (result.length >= MAX_SAVED_LAYOUTS) break
  }
  return result
}

/** 按视觉顺序（先 first 再 second）展平布局树，得到模块板上的模块顺序。 */
export function flattenLayout(layout: LayoutNode): ModulePlacement[] {
  if (layout.type === 'leaf') return [{ id: layout.id }]
  return [...flattenLayout(layout.first), ...flattenLayout(layout.second)]
}

/** 按视觉顺序返回布局树里的模块 id。 */
export function layoutIds(layout: LayoutNode): ModuleId[] {
  return flattenLayout(layout).map((placement) => placement.id)
}

interface LayoutDetach {
  removed: boolean
  node: LayoutNode | null
}

function detachLayoutModule(layout: LayoutNode, id: ModuleId): LayoutDetach {
  if (layout.type === 'leaf') {
    return layout.id === id ? { removed: true, node: null } : { removed: false, node: layout }
  }
  const first = detachLayoutModule(layout.first, id)
  if (first.removed) {
    // 子树被整个删除时用兄弟子树替代父 split，保持树紧凑；否则把新子树写回父节点。
    if (first.node === null) return { removed: true, node: layout.second }
    return { removed: true, node: { ...layout, first: first.node } }
  }
  const second = detachLayoutModule(layout.second, id)
  if (second.removed) {
    if (second.node === null) return { removed: true, node: layout.first }
    return { removed: true, node: { ...layout, second: second.node } }
  }
  return { removed: false, node: layout }
}

/**
 * 从布局树里删除一个模块：删除叶子并折叠父 split（用兄弟子树替代父节点）。
 * 删除根叶子返回 null；模块不存在时原样返回原树。
 */
export function removeLayoutModule(layout: LayoutNode, id: ModuleId): LayoutNode | null {
  const result = detachLayoutModule(layout, id)
  return result.removed ? result.node : layout
}

/**
 * 追加模块到模块板最右侧：把旧根包裹成 row split，first 是旧树、second 是新叶子，
 * ratio 取 旧叶子数 / (旧叶子数 + 1)，让新模块占总宽度的 1/n。
 * 已存在的模块不会重复添加，直接返回原树。
 */
export function appendLayoutModule(layout: LayoutNode | null, id: ModuleId): LayoutNode {
  if (!layout) return createLeaf(id)
  if (layoutHasModule(layout, id)) return layout
  const count = layoutLeafCount(layout)
  return createSplit('row', clampSplitRatio(count / (count + 1)), layout, createLeaf(id))
}

/**
 * 追加模块到模块板最左侧：把旧根包裹成 row split，first 是新叶子、second 是旧树，
 * ratio 取 1 / (旧叶子数 + 1)，让新模块占总宽度的 1/n。
 */
export function prependLayoutModule(layout: LayoutNode | null, id: ModuleId): LayoutNode {
  if (!layout) return createLeaf(id)
  if (layoutHasModule(layout, id)) return layout
  const count = layoutLeafCount(layout)
  return createSplit('row', clampSplitRatio(1 / (count + 1)), createLeaf(id), layout)
}

/**
 * 把模块移动到整个布局的最左或最右一列：先从树中移除，再用 prepend/append 包裹成新的 row split。
 * 模块不存在时原样返回。
 */
export function moveLayoutModuleToEdge(root: LayoutNode, draggedId: ModuleId, side: 'left' | 'right'): LayoutNode {
  if (!layoutHasModule(root, draggedId)) return root
  const remaining = removeLayoutModule(root, draggedId)
  if (!remaining) return root
  return side === 'left' ? prependLayoutModule(remaining, draggedId) : appendLayoutModule(remaining, draggedId)
}

function replaceLayoutLeaf(layout: LayoutNode, targetId: ModuleId, create: () => LayoutNode): LayoutNode | null {
  if (layout.type === 'leaf') return layout.id === targetId ? create() : null
  const first = replaceLayoutLeaf(layout.first, targetId, create)
  if (first) return { ...layout, first }
  const second = replaceLayoutLeaf(layout.second, targetId, create)
  if (second) return { ...layout, second }
  return null
}

/**
 * 把 dragged 模块拖到 target 模块的指定一侧：先删除 dragged，再把 target 叶子替换成新的分栏。
 * left/top 时 dragged 在 first；right/bottom 时 dragged 在 second；left/right 用 row，top/bottom 用 column。
 */
export function splitLayout(root: LayoutNode, draggedId: ModuleId, targetId: ModuleId, side: DropSide): LayoutNode {
  if (draggedId === targetId || !layoutHasModule(root, targetId)) return root
  const remaining = removeLayoutModule(root, draggedId)
  if (!remaining) return root
  const direction: SplitDirection = side === 'top' || side === 'bottom' ? 'column' : 'row'
  const dragged = createLeaf(draggedId)
  const target = createLeaf(targetId)
  const draggedFirst = side === 'left' || side === 'top'
  const replaced = replaceLayoutLeaf(remaining, targetId, () =>
    createSplit(direction, DEFAULT_SPLIT_RATIO, draggedFirst ? dragged : target, draggedFirst ? target : dragged),
  )
  return replaced ?? root
}

function splitAtPath(layout: LayoutNode, path: readonly number[]): LayoutSplit | null {
  let current: LayoutNode = layout
  for (const step of path) {
    if (current.type !== 'split') return null
    if (step === 0) current = current.first
    else if (step === 1) current = current.second
    else return null
  }
  return current.type === 'split' ? current : null
}

function updateSplitAtPath(layout: LayoutNode, path: readonly number[], ratio: number): LayoutNode {
  if (layout.type !== 'split') return layout
  if (!path.length) return { ...layout, ratio }
  const [step, ...rest] = path
  if (step === 0) return { ...layout, first: updateSplitAtPath(layout.first, rest, ratio) }
  return { ...layout, second: updateSplitAtPath(layout.second, rest, ratio) }
}

/** 按 0/1 路径更新某个 split 的比例并夹紧；路径无效时原样返回原树。 */
export function setSplitRatio(root: LayoutNode, path: number[], ratio: number): LayoutNode {
  const target = splitAtPath(root, path)
  if (!target) return root
  const clamped = clampSplitRatio(ratio)
  if (target.ratio === clamped) return root
  return updateSplitAtPath(root, path, clamped)
}

function isValidExtent(value: number): boolean {
  return Number.isFinite(value) && value > 0
}

interface GeometryResult {
  slots: LayoutSlot[]
  gutters: LayoutGutter[]
}

function walkLayoutGeometry(
  layout: LayoutNode,
  rect: Rect,
  path: number[],
  gap: number,
  overrides: ReadonlyMap<string, number> | undefined,
  result: GeometryResult,
): boolean {
  if (!isValidExtent(rect.width) || !isValidExtent(rect.height)) return false
  if (layout.type === 'leaf') {
    result.slots.push({
      id: layout.id,
      left: rect.left,
      top: rect.top,
      width: rect.width,
      height: rect.height,
    })
    return true
  }
  // 拖动分栏间隙时只在本组件内预览比例，几何计算同样用覆盖比例，槽位才能实时跟随指针。
  const ratio = overrides?.get(path.join('/')) ?? layout.ratio
  if (!(ratio > 0) || ratio >= 1) return false
  const isRow = layout.direction === 'row'
  const mainSize = isRow ? rect.width : rect.height
  const crossSize = isRow ? rect.height : rect.width
  const available = mainSize - gap
  if (!isValidExtent(available)) return false
  const firstSize = available * ratio
  const secondSize = available - firstSize
  if (!isValidExtent(firstSize) || !isValidExtent(secondSize)) return false
  const firstRect: Rect = isRow
    ? { left: rect.left, top: rect.top, width: firstSize, height: crossSize }
    : { left: rect.left, top: rect.top, width: crossSize, height: firstSize }
  const secondRect: Rect = isRow
    ? { left: rect.left + firstSize + gap, top: rect.top, width: secondSize, height: crossSize }
    : { left: rect.left, top: rect.top + firstSize + gap, width: crossSize, height: secondSize }
  const gutter: LayoutGutter = isRow
    ? {
        left: rect.left + firstSize,
        top: rect.top,
        width: gap,
        height: crossSize,
        path: [...path],
        direction: layout.direction,
        ratio,
        parent: { ...rect },
      }
    : {
        left: rect.left,
        top: rect.top + firstSize,
        width: crossSize,
        height: gap,
        path: [...path],
        direction: layout.direction,
        ratio,
        parent: { ...rect },
      }
  result.gutters.push(gutter)
  return (
    walkLayoutGeometry(layout.first, firstRect, [...path, 0], gap, overrides, result) &&
    walkLayoutGeometry(layout.second, secondRect, [...path, 1], gap, overrides, result)
  )
}

/**
 * 递归把布局树切成像素矩形：
 * split 的可用主轴长度 = 父尺寸 - gap，firstSize = (可用) * ratio，second 从 first 结束 + gap 开始；
 * 叶子返回 slot，每个 split 返回覆盖间隙的 gutter（path 为 0/1 路径，parent 为 split 所在矩形）。
 * 尺寸、gap 或比例非法时返回空几何，避免产生 NaN / 负尺寸。
 */
export function layoutGeometry(
  root: LayoutNode,
  width: number,
  height: number,
  gap: number,
  overrides?: ReadonlyMap<string, number>,
): LayoutGeometry {
  if (!isValidExtent(width) || !isValidExtent(height) || !Number.isFinite(gap) || gap < 0) {
    return { slots: [], gutters: [] }
  }
  const result: GeometryResult = { slots: [], gutters: [] }
  const valid = walkLayoutGeometry(root, { left: 0, top: 0, width, height }, [], gap, overrides, result)
  return valid ? result : { slots: [], gutters: [] }
}

/** 命中第一个包含该点、且不是正在拖拽模块的槽位。 */
export function findDropSlot(slots: LayoutSlot[], x: number, y: number, draggedId?: ModuleId): LayoutSlot | null {
  for (const slot of slots) {
    if (draggedId !== undefined && slot.id === draggedId) continue
    if (x >= slot.left && x <= slot.left + slot.width && y >= slot.top && y <= slot.top + slot.height) return slot
  }
  return null
}

/** 按指针相对槽位中心的归一化偏移判断落点方向：水平偏移不小于垂直偏移时取 left/right。 */
export function dropSideForPoint(slot: LayoutSlot, x: number, y: number): DropSide {
  const dx = slot.width > 0 ? (x - slot.left) / slot.width - 0.5 : 0
  const dy = slot.height > 0 ? (y - slot.top) / slot.height - 0.5 : 0
  if (Math.abs(dx) >= Math.abs(dy)) return dx >= 0 ? 'right' : 'left'
  return dy >= 0 ? 'bottom' : 'top'
}

/** 拖放预览矩形：目标槽位的对应一半（左右各半宽，上下各半高）。 */
export function dropPreviewRect(slot: LayoutSlot, side: DropSide): Rect {
  const halfWidth = slot.width / 2
  const halfHeight = slot.height / 2
  if (side === 'left') return { left: slot.left, top: slot.top, width: halfWidth, height: slot.height }
  if (side === 'right') return { left: slot.left + halfWidth, top: slot.top, width: halfWidth, height: slot.height }
  if (side === 'top') return { left: slot.left, top: slot.top, width: slot.width, height: halfHeight }
  return { left: slot.left, top: slot.top + halfHeight, width: slot.width, height: halfHeight }
}

interface BoundaryInfo {
  split: LayoutSplit
  /** split 在整棵树中占总可分配空间的比例。 */
  share: number
  leftLeaf: ModuleId
  rightLeaf: ModuleId
}

function rightmostLeaf(layout: LayoutNode): ModuleId {
  return layout.type === 'leaf' ? layout.id : rightmostLeaf(layout.second)
}

function leftmostLeaf(layout: LayoutNode): ModuleId {
  return layout.type === 'leaf' ? layout.id : leftmostLeaf(layout.first)
}

/** 找到 path 对应的 split、它在根坐标中的占比，以及紧邻间隙左右/上下的两个叶子。 */
function findBoundary(layout: LayoutNode, path: readonly number[], share: number): BoundaryInfo | null {
  if (layout.type !== 'split') return null
  if (!path.length) {
    return {
      split: layout,
      share,
      leftLeaf: rightmostLeaf(layout.first),
      rightLeaf: leftmostLeaf(layout.second),
    }
  }
  const [step, ...rest] = path
  if (step === 0) return findBoundary(layout.first, rest, share * layout.ratio)
  if (step === 1) return findBoundary(layout.second, rest, share * (1 - layout.ratio))
  return null
}

type ResizeEdge = 'start' | 'end'
interface ExtentBounds {
  min: number
  max: number
}

/** 固定子树的另一侧边界，计算只移动指定边缘时允许的尺寸范围。 */
function edgeResizeBounds(
  layout: LayoutNode,
  direction: SplitDirection,
  edge: ResizeEdge,
  extent: number,
  gap: number,
  minimum: number,
): ExtentBounds {
  if (layout.type === 'leaf') return { min: Math.min(extent, minimum), max: Infinity }
  if (layout.direction !== direction) {
    // 上下分栏共享左右边缘：沿同一侧同时调整两行，而非把整行按比例缩放。
    const first = edgeResizeBounds(layout.first, direction, edge, extent, gap, minimum)
    const second = edgeResizeBounds(layout.second, direction, edge, extent, gap, minimum)
    return { min: Math.max(first.min, second.min), max: Math.min(first.max, second.max) }
  }
  const available = extent - gap
  const first = available * layout.ratio
  const second = available - first
  const fixed = edge === 'end' ? first : second
  const moving = edge === 'end' ? layout.second : layout.first
  const bounds = edgeResizeBounds(moving, direction, edge, edge === 'end' ? second : first, gap, minimum)
  return {
    min: Math.max(gap + fixed + bounds.min, gap + fixed / MAX_SPLIT_RATIO),
    max: Math.min(gap + fixed + bounds.max, gap + fixed / MIN_SPLIT_RATIO),
  }
}

/** 从指定边缘吸收尺寸变化，同向分栏只调整贴边子树，垂直分栏同时调整两个子树。 */
function resizeFromEdge(
  layout: LayoutNode,
  direction: SplitDirection,
  edge: ResizeEdge,
  extent: number,
  nextExtent: number,
  gap: number,
): LayoutNode {
  if (layout.type === 'leaf') return layout
  if (layout.direction !== direction) {
    return {
      ...layout,
      first: resizeFromEdge(layout.first, direction, edge, extent, nextExtent, gap),
      second: resizeFromEdge(layout.second, direction, edge, extent, nextExtent, gap),
    }
  }
  const first = (extent - gap) * layout.ratio
  const second = extent - gap - first
  const nextAvailable = nextExtent - gap
  if (edge === 'end') {
    return {
      ...layout,
      ratio: clampSplitRatio(first / nextAvailable),
      second: resizeFromEdge(layout.second, direction, edge, second, nextAvailable - first, gap),
    }
  }
  return {
    ...layout,
    ratio: clampSplitRatio((nextAvailable - second) / nextAvailable),
    first: resizeFromEdge(layout.first, direction, edge, first, nextAvailable - second, gap),
  }
}

/** 只移动接触目标间隙的边缘；混合嵌套分栏中的非相邻模块保持原有位置和尺寸。 */
export function resizeLayoutLocally(
  root: LayoutNode,
  path: readonly number[],
  ratio: number,
  metrics: LayoutMetrics = { width: 1, height: 1, gap: 0 },
): LayoutNode {
  const target = splitAtPath(root, path)
  if (
    !target ||
    !isValidExtent(metrics.width) ||
    !isValidExtent(metrics.height) ||
    !Number.isFinite(metrics.gap) ||
    metrics.gap < 0
  )
    return root
  const { direction } = target
  const { gap } = metrics
  const rootExtent = direction === 'row' ? metrics.width : metrics.height
  let extent = rootExtent
  let node = root
  for (const step of path) {
    if (node.type !== 'split') return root
    if (node.direction === direction) extent = (extent - gap) * (step === 0 ? node.ratio : 1 - node.ratio)
    node = step === 0 ? node.first : node.second
  }
  const available = extent - gap
  if (!isValidExtent(available)) return root
  const first = available * target.ratio
  const second = available - first
  const firstBounds = edgeResizeBounds(target.first, direction, 'end', first, gap, rootExtent * MIN_SPLIT_RATIO)
  const secondBounds = edgeResizeBounds(target.second, direction, 'start', second, gap, rootExtent * MIN_SPLIT_RATIO)
  const lower = Math.max(firstBounds.min - first, second - secondBounds.max)
  const upper = Math.min(firstBounds.max - first, second - secondBounds.min)
  if (lower > upper) return root
  const requested = (clampSplitRatio(ratio) - target.ratio) * available
  const delta = Math.max(lower, Math.min(upper, requested))
  if (!Number.isFinite(delta) || Math.abs(delta) < 1e-6) return root
  const resized: LayoutSplit = {
    ...target,
    ratio: clampSplitRatio((first + delta) / available),
    first: resizeFromEdge(target.first, direction, 'end', first, first + delta, gap),
    second: resizeFromEdge(target.second, direction, 'start', second, second - delta, gap),
  }
  return replaceLayoutNode(root, target, resized)
}

function findSplitByBoundary(layout: LayoutNode, leftLeaf: ModuleId, rightLeaf: ModuleId): LayoutSplit | null {
  if (layout.type === 'leaf') return null
  if (rightmostLeaf(layout.first) === leftLeaf && leftmostLeaf(layout.second) === rightLeaf) return layout
  return (
    findSplitByBoundary(layout.first, leftLeaf, rightLeaf) ?? findSplitByBoundary(layout.second, leftLeaf, rightLeaf)
  )
}

function replaceLayoutNode(layout: LayoutNode, target: LayoutNode, replacement: LayoutNode): LayoutNode {
  if (layout === target) return replacement
  if (layout.type === 'leaf') return layout
  return {
    ...layout,
    first: replaceLayoutNode(layout.first, target, replacement),
    second: replaceLayoutNode(layout.second, target, replacement),
  }
}

/**
 * 把模块插入到指定间隙（滑块）处，成为原本相邻两个模块之间的第三个分栏。
 * 先按间隙左右/上下边界叶子定位原 split，再从布局中移除被拖动模块；
 * 新布局保持 split 方向，三个区域各占该 split 的 1/3（二进制表示：1/3 + 剩余 2/3 再对半）。
 * 拖动模块本身就是间隙某一侧时视为无效移动，原样返回。
 */
export function insertLayoutModuleAtGutter(root: LayoutNode, draggedId: ModuleId, path: readonly number[]): LayoutNode {
  const boundary = findBoundary(root, path, 1)
  if (!boundary || draggedId === boundary.leftLeaf || draggedId === boundary.rightLeaf) return root
  if (!layoutHasModule(root, draggedId)) return root
  const remaining = removeLayoutModule(root, draggedId)
  if (!remaining) return root
  const target = findSplitByBoundary(remaining, boundary.leftLeaf, boundary.rightLeaf)
  if (!target) return root
  const dragged = createLeaf(draggedId)
  const inserted = createSplit(
    target.direction,
    1 / 3,
    target.first,
    createSplit(target.direction, 0.5, dragged, target.second),
  )
  return replaceLayoutNode(remaining, target, inserted)
}
