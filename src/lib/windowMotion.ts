/** 模块最大化与设置窗口共用的动效节奏。 */
export const MAXIMIZE_DURATION = 360
export const RESTORE_DURATION = 280
export const MAXIMIZE_EASING = 'cubic-bezier(0.2, 0.8, 0.2, 1)'
export const RESTORE_EASING = 'cubic-bezier(0.4, 0, 0.2, 1)'

export interface WindowMotionRect {
  left: number
  top: number
  width: number
  height: number
}

/** 将完整窗口映射到按钮或当前动画中的屏幕矩形。 */
export function windowRectTransform(rect: WindowMotionRect, window: WindowMotionRect): string | null {
  if (
    ![rect.left, rect.top, rect.width, rect.height, window.left, window.top, window.width, window.height].every(
      Number.isFinite,
    ) ||
    rect.width <= 0 ||
    rect.height <= 0 ||
    window.width <= 0 ||
    window.height <= 0
  )
    return null
  return `translate(${rect.left - window.left}px, ${rect.top - window.top}px) scale(${rect.width / window.width}, ${rect.height / window.height})`
}
