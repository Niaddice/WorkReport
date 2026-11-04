export interface GitCredentials {
  url: string
  username: string
  password: string
  save?: boolean
}

export function isGithubHttpsOrigin(value: string): boolean {
  try {
    const url = new URL(value)
    return (
      url.origin === 'https://github.com' &&
      !url.username &&
      !url.password &&
      url.pathname === '/' &&
      !url.search &&
      !url.hash
    )
  } catch {
    return false
  }
}

export function gitErrorMessage(cause: unknown) {
  const raw = cause instanceof Error ? cause.message : String(cause)
  return raw.replace(/^(?:Error:\s*)?(?:Git 操作失败：\s*)+/, '').trim()
}

/** Git reports the credential context in these errors; discard any URL userinfo. */
export function gitAuthenticationOrigin(cause: unknown): string | null {
  const message = gitErrorMessage(cause)
  const match = message.match(
    /(?:could not read (?:Username|Password) for|Authentication failed for)\s+['"](https?:\/\/[^'"\s]+)['"]/i,
  )
  if (!match) return null
  try {
    const url = new URL(match[1])
    return url.origin
  } catch {
    return null
  }
}
