// A check of the pg_dump / pg_restore version list and the install guide. Run: bun test
import { expect, test } from 'bun:test'
import { PgInstallGuide } from '../src/services/PgInstallGuide'
import { PgToolChoice, PgToolLoader } from '../src/services/PgToolChoice'
import type { PgTools } from '../src/types'

function tool(
  version: number,
  release: string,
): { version: number; release: string; path: string } {
  return { version, release, path: `/pg${version}/pg_dump` }
}

test('the list shows PostgreSQL 18 to 12 and marks the missing ones', function () {
  const tools = [tool(17, '17.6'), tool(15, '15.10')]
  const choice = new PgToolChoice('pg_dump', { server: 17, tools, defaultVersion: 17 })
  expect(choice.options.map(choice.label, choice)).toEqual([
    'PostgreSQL 18 (not installed)',
    'PostgreSQL 17.6',
    'PostgreSQL 16 (not installed)',
    'PostgreSQL 15.10',
    'PostgreSQL 14 (not installed)',
    'PostgreSQL 13 (not installed)',
    'PostgreSQL 12 (not installed)',
  ])
  expect(choice.version).toBe(17)
  expect(choice.installed).toBe(true)
  expect(choice.missing).toBe('')
  expect(choice.warning).toBe('')

  choice.version = 15
  expect(choice.warning).toBe(
    'pg_dump 15 is older than the server (PostgreSQL 17). pg_dump does not back up a newer server.',
  )
  choice.version = 16
  expect(choice.installed).toBe(false)
  expect(choice.warning).toBe('')
})

test('an installed version outside the list is listed too, in order', function () {
  const list: PgTools = { server: 17, tools: [tool(11, '11.22')], defaultVersion: 11 }
  const choice = new PgToolChoice('pg_dump', list)
  const versions = choice.options.map(function version(option) {
    return option.version
  })
  expect(versions).toEqual([18, 17, 16, 15, 14, 13, 12, 11])
  expect(choice.version).toBe(11)
  expect(choice.installed).toBe(true)
})

test('with no tool installed it picks the server version and shows the guide', function () {
  const none = new PgToolChoice('pg_restore', { server: 16, tools: [], defaultVersion: null })
  expect(none.version).toBe(16)
  expect(none.installed).toBe(false)
  expect(none.missing).toBe('No pg_restore was found. Install one as shown below.')
  const old = new PgToolChoice('pg_restore', { server: 9, tools: [], defaultVersion: null })
  expect(old.version).toBe(18)
})

test('the install guide fits the platform', function () {
  const api = { openLink: async function openLink(): Promise<void> {} }
  const mac = new PgInstallGuide('MacIntel', api).steps(16)
  expect(mac[0]!.command).toBe('brew install postgresql@16')
  expect(mac[1]!.text).toContain('If Homebrew no longer has it')
  const oldMac = new PgInstallGuide('MacIntel', api).steps(14)
  expect(oldMac.length).toBe(1)
  expect(oldMac[0]!.command).toBe('')
  expect(oldMac[0]!.text).toBe('Use Postgres.app or the EDB installer.')
  expect(new PgInstallGuide('MacIntel', api).steps(15)[0]!.command).toBe(
    'brew install postgresql@15',
  )

  const linux = new PgInstallGuide('Linux x86_64', api).steps(15)
  expect(linux[0]!.command).toBe('sudo apt install postgresql-client-15')
  expect(linux[0]!.links[0]!.url).toBe('https://wiki.postgresql.org/wiki/Apt')
  expect(linux[1]!.command).toBe('sudo dnf install postgresql15')
  expect(linux[2]!.text).toContain('PGDG archive')

  const windows = new PgInstallGuide('Win32', api).steps(17)
  expect(windows[0]!.command).toBe('')
  expect(windows[0]!.text).toContain('Command Line Tools')
  expect(windows[0]!.links[0]!.url).toBe('https://www.postgresql.org/download/windows/')
})

test('the guide opens its links through the api', async function () {
  const opened: string[] = []
  const api = {
    openLink: async function openLink(url: string): Promise<void> {
      opened.push(url)
    },
  }
  const guide = new PgInstallGuide('Win32', api)
  await guide.open(guide.steps(17)[0]!.links[0]!)
  expect(opened).toEqual(['https://www.postgresql.org/download/windows/'])
})

/** Answers each pgTools call only when the test says so. */
class SlowSource {
  private answers: ((list: PgTools) => void)[] = []

  pgTools(): Promise<PgTools> {
    return new Promise<PgTools>(this.wait.bind(this))
  }

  private wait(resolve: (list: PgTools) => void): void {
    this.answers.push(resolve)
  }

  answer(index: number, list: PgTools): void {
    this.answers[index]!(list)
  }
}

test('a slow answer from an older open is dropped', async function () {
  const source = new SlowSource()
  const loader = new PgToolLoader()
  const first = loader.load(source, false)
  const second = loader.load(source, false)
  source.answer(1, { server: 17, tools: [tool(17, '17.6')], defaultVersion: 17 })
  source.answer(0, { server: 17, tools: [], defaultVersion: null })
  expect(await first).toBeNull()
  expect((await second)?.version).toBe(17)

  const late = loader.load(source, true)
  loader.drop()
  source.answer(2, { server: 17, tools: [], defaultVersion: null })
  expect(await late).toBeNull()
})
