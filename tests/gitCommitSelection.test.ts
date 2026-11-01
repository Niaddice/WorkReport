import { afterEach, describe, expect, it } from 'vitest'
import { effectScope, ref, type EffectScope } from 'vue'
import {
  MAX_SELECTED_COMMITS,
  selectedCommitInformation,
  selectedCommitReport,
  useGitCommitSelection,
} from '../src/lib/gitCommitSelection'
import type { GitHistoryCommit } from '../src/lib/useGitWorkbench'

const scopes: EffectScope[] = []
afterEach(() => scopes.splice(0).forEach((scope) => scope.stop()))
function commit(hash: string, committedAt = '2026-11-01T10:00:00+08:00'): GitHistoryCommit {
  return {
    hash,
    committedAt,
    parents: ['parent'],
    author: '作者',
    email: 'dev@example.test',
    message: `完成 ${hash}\n\n详细说明`,
    refs: 'main',
  }
}
function setup() {
  const workspacePath = ref('/workspace')
  const repositories = ref([
    { id: 'one', name: '项目一', path: '/projects/one', author: '', branch: '' },
    { id: 'two', name: '项目二', path: '/projects/two', author: '', branch: '' },
  ])
  const scope = effectScope()
  scopes.push(scope)
  return {
    workspacePath,
    repositories,
    ...scope.run(() =>
      useGitCommitSelection(() => ({ workspacePath: workspacePath.value, repositories: repositories.value })),
    )!,
  }
}

describe('cross-repository commit selection', () => {
  it('deduplicates within a repository and retains the same hash in a different repository', () => {
    const selection = setup()
    selection.set('one', [commit('same'), commit('same')], true)
    selection.set('two', [commit('same')], true)
    expect(selection.entries.value).toHaveLength(2)
    expect(selection.repositoryCount.value).toBe(2)
    selection.clear('one')
    expect(selection.has('one', 'same')).toBe(false)
    expect(selection.has('two', 'same')).toBe(true)
  })

  it('takes detached snapshots and updates repository names without losing selections', () => {
    const selection = setup()
    const original = commit('first')
    selection.set('one', [original], true)
    original.message = 'later change'
    original.parents.push('other')
    selection.repositories.value[0]!.name = '改名后的项目'
    expect(selection.entries.value[0]!.repository.name).toBe('改名后的项目')
    expect(selection.entries.value[0]!.commit.message).toContain('完成 first')
    expect(selection.entries.value[0]!.commit.parents).toEqual(['parent'])
  })

  it('drops removed or moved repositories while preserving selections from other repositories', () => {
    const selection = setup()
    selection.set('one', [commit('one')], true)
    selection.set('two', [commit('two')], true)
    selection.repositories.value[0]!.path = '/projects/new-one'
    expect(selection.entries.value.map((entry) => entry.commit.hash)).toEqual(['two'])
    selection.repositories.value = selection.repositories.value.filter((repo) => repo.id !== 'two')
    expect(selection.entries.value).toHaveLength(0)
  })

  it('clears selections when switching workspaces even if the repositories have the same IDs', () => {
    const selection = setup()
    selection.set('one', [commit('old')], true)
    selection.workspacePath.value = '/other-workspace'
    expect(selection.entries.value).toHaveLength(0)
    expect(selection.set('missing', [commit('invalid')], true)).toBe(false)
  })

  it('bounds bulk selection without partially adding an oversized batch', () => {
    const selection = setup()
    const commits = Array.from({ length: MAX_SELECTED_COMMITS }, (_, i) => commit(String(i)))
    expect(selection.set('one', commits, true)).toBe(true)
    expect(selection.set('two', [commit('extra')], true)).toBe(false)
    expect(selection.entries.value).toHaveLength(MAX_SELECTED_COMMITS)
    expect(selection.error.value).toContain('500')
    selection.set('one', [commits[0]!], false)
    expect(selection.set('two', [commit('extra')], true)).toBe(true)
    expect(selection.error.value).toBe('')
  })

  it('groups repositories, orders by actual timestamps and preserves complete original messages', () => {
    const selection = setup()
    selection.set(
      'one',
      [commit('older', '2026-11-01T12:00:00+09:00'), commit('newer', '2026-11-01T11:00:00+07:00')],
      true,
    )
    selection.set('two', [{ ...commit('literal'), message: '<script>literal</script>\n说明' }], true)
    const information = selectedCommitInformation(selection.entries.value)
    expect(information.indexOf('提交：newer')).toBeLessThan(information.indexOf('提交：older'))
    expect(information).toContain('/projects/one')
    expect(information).toContain('作者：作者 <dev@example.test>')
    expect(information).toContain('父提交：parent')
    expect(information).toContain('完成 older\n\n详细说明')
    const report = selectedCommitReport(selection.entries.value)
    expect(report).toContain('项目一\n1、完成 newer')
    expect(report).toContain('项目二\n1、<script>literal</script>')
    expect(report).not.toContain('提交：')
  })
})
