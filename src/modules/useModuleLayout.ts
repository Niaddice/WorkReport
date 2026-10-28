import { computed, ref } from 'vue'
import { uiPreferencesApi } from '../lib/uiPreferences'
import {
  DEFAULT_UI_PREFERENCES,
  MAX_SAVED_LAYOUTS,
  appendLayoutModule,
  cloneUiPreferences,
  flattenLayout,
  insertLayoutModuleAtGutter,
  moveLayoutModuleToEdge,
  normalizeUiPreferences,
  removeLayoutModule,
  resizeLayoutLocally,
  splitLayout,
} from './layout'
import type { SkinId } from '../lib/skins'
import type { DropSide, LayoutMetrics, ModuleId, UiPreferences } from './types'

export interface ModuleLayoutOptions {
  /** 读取或保存布局失败时通知页面编排层显示错误。 */
  onError?: (message: string) => void
  /** 保存时携带当前工作空间路径；后端会校验，避免旧布局写进新空间。 */
  workspacePath?: () => string | null
}

/**
 * 模块化首页的布局状态与持久化。
 *
 * - 布局是一棵可嵌套的分栏树，叶子是模块；
 * - 添加模块追加到最右侧，拖放到其他模块上会生成左右/上下分栏；
 * - 分栏间隙可拖动改变比例；
 * - 所有修改先归一化再写入工作空间的 workreport.json；
 * - 工作空间切换时用 revision 丢弃迟到的读写，避免旧布局写进新空间。
 */
export function useModuleLayout(options: ModuleLayoutOptions = {}) {
  const preferences = ref<UiPreferences>(cloneUiPreferences(DEFAULT_UI_PREFERENCES))
  const loading = ref(false)
  const saving = ref(false)
  const error = ref('')
  const layout = computed(() => preferences.value.layout)
  const items = computed(() => flattenLayout(preferences.value.layout))
  const skin = computed(() => preferences.value.skin)
  const savedLayouts = computed(() => preferences.value.savedLayouts)
  let revision = 0
  let pendingSaves = 0
  let saveChain: Promise<void> = Promise.resolve()

  function report(cause: unknown) {
    const message = `保存界面布局失败：${String(cause)}`
    error.value = message
    options.onError?.(message)
  }

  function resetLocal() {
    revision++
    preferences.value = normalizeUiPreferences(DEFAULT_UI_PREFERENCES)
    error.value = ''
  }

  async function load(): Promise<boolean> {
    const token = ++revision
    loading.value = true
    error.value = ''
    try {
      const loaded = await uiPreferencesApi.load()
      if (token !== revision) return false
      preferences.value = normalizeUiPreferences(loaded)
      return true
    } catch (cause) {
      if (token !== revision) return false
      preferences.value = normalizeUiPreferences(DEFAULT_UI_PREFERENCES)
      const message = `读取界面布局失败：${String(cause)}`
      error.value = message
      options.onError?.(message)
      return false
    } finally {
      if (token === revision) loading.value = false
    }
  }

  function persist() {
    const token = revision
    const workspacePath = options.workspacePath?.() ?? null
    if (!workspacePath) return
    const snapshot = cloneUiPreferences(preferences.value)
    pendingSaves++
    saving.value = true
    saveChain = saveChain
      .then(async () => {
        // 排队期间切换了工作空间或又有更新的布局，就丢弃这次旧快照。
        if (token !== revision || workspacePath !== (options.workspacePath?.() ?? null)) return
        try {
          await uiPreferencesApi.save(snapshot, workspacePath)
        } catch (cause) {
          if (token === revision && workspacePath === (options.workspacePath?.() ?? null)) report(cause)
        }
      })
      .finally(() => {
        pendingSaves--
        if (pendingSaves === 0) saving.value = false
      })
  }

  function update(next: UiPreferences) {
    preferences.value = normalizeUiPreferences(next)
    error.value = ''
    persist()
  }

  /** 追加到模块板最右侧；已有模块不重复添加。 */
  function add(id: ModuleId) {
    if (items.value.some((placement) => placement.id === id)) return
    update({ ...preferences.value, layout: appendLayoutModule(preferences.value.layout, id) })
  }

  function remove(id: ModuleId) {
    // 所有模块都可关闭，但必须至少保留一个模块。
    if (items.value.length <= 1) return
    const next = removeLayoutModule(preferences.value.layout, id)
    if (!next) return
    update({ ...preferences.value, layout: next })
  }

  /** 把 dragged 拖到 target 的左/右/上/下侧，形成新的分栏。 */
  function split(dragged: ModuleId, target: ModuleId, side: DropSide): boolean {
    if (dragged === target) return false
    const next = splitLayout(preferences.value.layout, dragged, target, side)
    if (next === preferences.value.layout) return false
    update({ ...preferences.value, layout: next })
    return true
  }

  /** 把模块拖到两个模块之间的间隙，插入到该位置。 */
  function insertAtGutter(dragged: ModuleId, path: number[]): boolean {
    const next = insertLayoutModuleAtGutter(preferences.value.layout, dragged, path)
    if (next === preferences.value.layout) return false
    update({ ...preferences.value, layout: next })
    return true
  }

  /** 把模块拖到整个模块板的最左或最右一列。 */
  function moveToEdge(dragged: ModuleId, side: 'left' | 'right'): boolean {
    const next = moveLayoutModuleToEdge(preferences.value.layout, dragged, side)
    if (next === preferences.value.layout) return false
    update({ ...preferences.value, layout: next })
    return true
  }

  /** 把当前布局存成命名快照；同名覆盖，超出数量上限时移除最旧的快照。 */
  function saveLayout(name: string): boolean {
    const trimmed = name.trim()
    if (!trimmed) return false
    const next = preferences.value.savedLayouts.filter((item) => item.name !== trimmed)
    next.push({ name: trimmed, layout: preferences.value.layout })
    if (next.length > MAX_SAVED_LAYOUTS) next.shift()
    update({ ...preferences.value, savedLayouts: next })
    return true
  }

  /** 切换到某个已保存的布局快照。 */
  function applySavedLayout(name: string): boolean {
    const saved = preferences.value.savedLayouts.find((item) => item.name === name)
    if (!saved) return false
    update({ ...preferences.value, layout: saved.layout })
    return true
  }

  /** 删除某个已保存的布局快照。 */
  function removeSavedLayout(name: string): boolean {
    const next = preferences.value.savedLayouts.filter((item) => item.name !== name)
    if (next.length === preferences.value.savedLayouts.length) return false
    update({ ...preferences.value, savedLayouts: next })
    return true
  }

  /** 拖动分栏间隙后更新比例。path 是 split 在树中的 0/1 路径。 */
  function resize(path: number[], ratio: number, metrics?: LayoutMetrics) {
    const next = resizeLayoutLocally(preferences.value.layout, path, ratio, metrics)
    if (next === preferences.value.layout) return
    update({ ...preferences.value, layout: next })
  }

  function setSkin(value: SkinId) {
    update({ ...preferences.value, skin: value })
  }

  function reset() {
    update(normalizeUiPreferences(DEFAULT_UI_PREFERENCES))
  }

  return {
    layout,
    items,
    skin,
    savedLayouts,
    load,
    resetLocal,
    add,
    remove,
    split,
    insertAtGutter,
    moveToEdge,
    saveLayout,
    applySavedLayout,
    removeSavedLayout,
    resize,
    setSkin,
    reset,
  }
}
