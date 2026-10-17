// Checks the process methods of the Session, with a fake backend. Run: bun test
import { expect, test } from 'bun:test'
import { DEFAULT_CONFIGS } from '../src/constants'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import { Session } from '../src/services/Session'
import { FakePrompt } from './FakePrompt'
import type { DbProcess, TableRef, UserList } from '../src/types'

const OWN: DbProcess = {
  id: 7,
  user: 'root',
  host: 'localhost',
  database: 'shop',
  state: 'Query: executing',
  seconds: 0,
  query: 'SELECT 1',
  current: true,
}
const SLOW: DbProcess = { ...OWN, id: 9, user: 'bob', seconds: 120, current: false }

class FakeApi {
  lists = 0
  calls: unknown[][] = []
  processes: DbProcess[] = [OWN, SLOW]

  async connect(): Promise<void> {}

  async disconnect(): Promise<void> {}

  async listDatabases(): Promise<string[]> {
    return ['shop']
  }

  async listTables(): Promise<TableRef[]> {
    return []
  }

  async listUsers(): Promise<UserList> {
    return { canManage: false, users: [] }
  }

  async listProcesses(): Promise<DbProcess[]> {
    this.lists += 1
    return this.processes
  }

  async killProcess(id: string, process: number, queryOnly: boolean): Promise<void> {
    if (process === OWN.id) {
      throw 'This is the connection of this tab. It cannot be stopped here.'
    }
    this.calls.push([id, process, queryOnly])
    this.processes = [OWN]
  }
}

async function connected(api: FakeApi): Promise<Session> {
  const session = new Session(api as unknown as DatabaseApi, new FakePrompt())
  await session.connect(DEFAULT_CONFIGS.mysql, 'test')
  return session
}

test('loadProcesses fills the list', async function () {
  const api = new FakeApi()
  const session = await connected(api)
  expect(session.processes).toEqual([])
  await session.loadProcesses()
  expect(session.processes).toEqual([OWN, SLOW])
})

test('refresh loads the processes again in the Processes view', async function () {
  const api = new FakeApi()
  const session = await connected(api)
  session.view = 'processes'
  await session.refresh()
  expect(api.lists).toBe(1)
})

test('killProcess sends the id and the mode, then loads the list again', async function () {
  const api = new FakeApi()
  const session = await connected(api)

  await session.killProcess(SLOW, true)
  expect(session.status).toBe('Query of process 9 stopped')
  await session.killProcess(SLOW, false)
  expect(session.status).toBe('Process 9 killed')

  expect(api.calls).toEqual([
    [session.id, 9, true],
    [session.id, 9, false],
  ])
  expect(api.lists).toBe(2)
  expect(session.processes).toEqual([OWN])
})

test('an error of the backend is shown', async function () {
  const session = await connected(new FakeApi())
  expect(await session.run(session.killProcess(OWN, false))).toBe(false)
  expect(session.error).toBe('This is the connection of this tab. It cannot be stopped here.')
})

test('disconnect clears the list', async function () {
  const session = await connected(new FakeApi())
  await session.loadProcesses()
  await session.disconnect()
  expect(session.processes).toEqual([])
})
