import { describe, expect, it } from 'vitest'
import { gitAuthenticationOrigin, gitErrorMessage } from '../src/lib/gitAuthentication'

describe('Git authentication errors', () => {
  it.each([
    "fatal: could not read Username for 'http://106.75.23.137:13000': terminal prompts disabled",
    "fatal: could not read Password for 'http://user@106.75.23.137:13000': terminal prompts disabled",
    "fatal: Authentication failed for 'http://user:secret@106.75.23.137:13000/team/repo.git/'",
  ])('extracts only the server origin from %s', (message) => {
    expect(gitAuthenticationOrigin(new Error(`Git 操作失败：${message}`))).toBe('http://106.75.23.137:13000')
  })

  it('does not treat branch conflicts or SSH errors as an HTTP login request', () => {
    expect(gitAuthenticationOrigin('fatal: non-fast-forward')).toBeNull()
    expect(gitAuthenticationOrigin("fatal: could not read Username for 'ssh://example.test'")).toBeNull()
    expect(gitAuthenticationOrigin('Permission denied (publickey)')).toBeNull()
  })

  it('removes repeated error prefixes', () => {
    expect(gitErrorMessage(new Error('Git 操作失败：Git 操作失败：fatal: denied'))).toBe('fatal: denied')
    expect(gitErrorMessage('普通错误')).toBe('普通错误')
  })
})
