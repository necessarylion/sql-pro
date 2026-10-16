// The Ask AI loop, with a fake backend. Run: bun test tests/AiChat.test.ts
import { expect, test } from 'bun:test'
import { AiChat } from '../src/services/AiChat'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import type { DiscardPrompt } from '../src/services/DiscardPrompt'
import {
  AI_MAX_CHATS,
  AI_MAX_STEPS,
  AI_READ_ONLY_REJECTED,
  AI_REJECTED,
  AI_RESULT_ROWS,
  AI_SESSION_GONE,
} from '../src/constants/ai'
import { AiStore } from '../src/services/AiStore'
import { ProfileStorage } from '../src/services/ProfileStorage'
import type { AiOptions, ClaudeReply, ResultSet, SavedChat } from '../src/types'
import { FakePrompt } from './FakePrompt'

const ID = 'tab'
const ASK: AiOptions = { provider: 'claude', model: null, effort: null, readOnly: false }
const READ_ONLY: AiOptions = { ...ASK, readOnly: true }

/** Gives the planned replies one by one. Keeps what was sent to the AI and what ran. */
class FakeApi {
  replies: ClaudeReply[] = []
  sent: { message: string; sessionId: string | null }[] = []
  ran: string[] = []
  result: ResultSet = { columns: ['id'], rows: [['1']], affected: 0, truncated: false }

  options: AiOptions[] = []
  /** Session ids that the AI tool has deleted. */
  gone: string[] = []

  async askClaude(
    _id: string,
    message: string,
    sessionId: string | null,
    options: AiOptions,
  ): Promise<ClaudeReply> {
    if (sessionId && this.gone.includes(sessionId)) {
      throw `${AI_SESSION_GONE}`
    }
    this.sent.push({ message, sessionId })
    this.options.push(options)
    return this.replies.shift() ?? { sessionId: 's1', message: 'Done', sql: null, isRead: false }
  }

  async runQuery(_id: string, sql: string): Promise<ResultSet[]> {
    this.ran.push(sql)
    if (sql === 'BAD') {
      throw 'no such table: x'
    }
    return [this.result]
  }
}

function reply(sql: string | null, isRead: boolean): ClaudeReply {
  return { sessionId: 's1', message: 'Working', sql, isRead }
}

function chatWith(api: FakeApi, prompt: FakePrompt): AiChat {
  return new AiChat(api as unknown as DatabaseApi, prompt as unknown as DiscardPrompt)
}

test('a read runs at once and its result goes back to the AI', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  api.replies = [reply('SELECT id FROM users', true)]
  const chat = chatWith(api, prompt)

  await chat.ask(ID, 'how many users?', ASK)

  expect(prompt.questions).toEqual([])
  expect(api.ran).toEqual(['SELECT id FROM users'])
  expect(api.sent[0]).toEqual({ message: 'how many users?', sessionId: null })
  expect(api.sent[1].sessionId).toBe('s1')
  expect(api.sent[1].message).toContain('Columns: id')
  expect(chat.result?.rows).toEqual([['1']])
  expect(chat.wrote).toBe(false)
  expect(chat.phase).toBe('')
})

test('a write asks first and runs on Yes', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  api.replies = [reply('UPDATE users SET blocked = 1', false)]
  api.result = { columns: [], rows: [], affected: 1, truncated: false }
  const chat = chatWith(api, prompt)

  await chat.ask(ID, 'block abc@email.com', ASK)

  expect(prompt.questions.length).toBe(1)
  expect(prompt.questions[0]).toContain('UPDATE users SET blocked = 1')
  expect(api.ran).toEqual(['UPDATE users SET blocked = 1'])
  expect(api.sent[1].message).toBe('OK. 1 rows affected.')
  expect(chat.wrote).toBe(true)
})

test('a rejected write does not run and the AI is told', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  prompt.answer = false
  api.replies = [reply('DELETE FROM users', false)]
  const chat = chatWith(api, prompt)

  await chat.ask(ID, 'delete everyone', ASK)

  expect(api.ran).toEqual([])
  expect(api.sent[1].message).toBe(AI_REJECTED)
  const sql = chat.entries.find(isSql)
  expect(sql?.outcome).toBe('Rejected')
  expect(chat.wrote).toBe(false)
})

test('a failed query sends the error back to the AI', async function () {
  const api = new FakeApi()
  api.replies = [reply('BAD', true)]
  const chat = chatWith(api, new FakePrompt())

  await chat.ask(ID, 'x', ASK)

  expect(api.sent[1].message).toContain('no such table: x')
  expect(chat.entries.find(isSql)?.failed).toBe(true)
})

test('the loop stops after the most steps', async function () {
  const api = new FakeApi()
  for (let index = 0; index < AI_MAX_STEPS + 5; index += 1) {
    api.replies.push(reply('SELECT 1', true))
  }
  const chat = chatWith(api, new FakePrompt())

  await chat.ask(ID, 'loop forever', ASK)

  expect(api.ran.length).toBe(AI_MAX_STEPS)
  expect(chat.entries[chat.entries.length - 1].kind).toBe('note')
})

test('clear starts a new conversation', async function () {
  const api = new FakeApi()
  const chat = chatWith(api, new FakePrompt())
  await chat.ask(ID, 'one', ASK)
  chat.clear()
  await chat.ask(ID, 'two', ASK)

  expect(api.sent[1]).toEqual({ message: 'two', sessionId: null })
  expect(chat.entries.length).toBe(2)
})

test('only the first rows go to the AI', function () {
  const rows: string[][] = []
  for (let index = 0; index < AI_RESULT_ROWS + 10; index += 1) {
    rows.push([String(index)])
  }
  const text = AiChat.describe({ columns: ['n'], rows, affected: 0, truncated: true }, 0)

  expect(text).toContain(`more than ${AI_RESULT_ROWS + 10}`)
  expect(text.split('\n').length).toBe(AI_RESULT_ROWS + 2)
  expect(
    AiChat.describe({ columns: ['a'], rows: [[null]], affected: 0, truncated: false }, 0),
  ).toContain('NULL')
})

function isSql(entry: { kind: string }): boolean {
  return entry.kind === 'sql'
}

test('read-only mode never runs a write and never asks, and the AI is told', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  api.replies = [reply('DELETE FROM users', false), reply('SELECT 1', true)]
  const chat = chatWith(api, prompt)

  await chat.ask(ID, 'delete everyone', READ_ONLY)

  expect(prompt.questions).toEqual([])
  expect(api.ran).toEqual(['SELECT 1'])
  expect(api.sent[1].message).toBe(AI_READ_ONLY_REJECTED)
  expect(api.options[0].readOnly).toBe(true)
  expect(chat.entries.find(isSql)?.failed).toBe(true)
  expect(chat.wrote).toBe(false)
})

test('a chat keeps its own provider', async function () {
  const api = new FakeApi()
  const chat = chatWith(api, new FakePrompt())
  await chat.ask(ID, 'one', ASK)
  await chat.ask(ID, 'two', { ...ASK, provider: 'other' })

  expect(api.options[1].provider).toBe('claude')
})

test('a saved chat is loaded and goes on in its session', async function () {
  const storage = new ProfileStorage(new FakeStorage() as unknown as Storage, '')
  const store = new AiStore(storage)
  const api = new FakeApi()
  api.replies = [reply('SELECT id FROM users', true)]
  const chat = chatWith(api, new FakePrompt())
  await chat.ask(ID, 'how many users?', ASK)
  const saved = chat.snapshot('sqlite:/a.db', 'a')
  expect(saved).not.toBeNull()
  store.save(saved as SavedChat)

  const loaded = new AiStore(storage).chatsOf('sqlite:/a.db')
  expect(loaded.length).toBe(1)
  expect(loaded[0].title).toBe('how many users?')
  expect(new AiStore(storage).chatsOf('other')).toEqual([])

  const again = chatWith(api, new FakePrompt())
  again.restore(loaded[0])
  await again.ask(ID, 'and now?', ASK)
  expect(api.sent[api.sent.length - 1].sessionId).toBe('s1')
  expect(again.entries.length).toBe(loaded[0].entries.length + 2)
})

test('result rows are never saved', async function () {
  const api = new FakeApi()
  api.result = { columns: ['email'], rows: [['secret@x.com']], affected: 0, truncated: false }
  api.replies = [reply('SELECT email FROM users', true)]
  const chat = chatWith(api, new FakePrompt())
  await chat.ask(ID, 'list emails', ASK)
  api.replies = []

  expect(chat.result?.rows).toEqual([['secret@x.com']])
  const text = JSON.stringify(chat.snapshot('c', 'd'))
  expect(text).not.toContain('secret@x.com')
})

test('only the newest chats are kept', function () {
  const store = new AiStore(new ProfileStorage(new FakeStorage() as unknown as Storage, ''))
  for (let index = 0; index < AI_MAX_CHATS + 5; index += 1) {
    store.save(savedChat(`chat${index}`, index))
  }

  expect(store.chats.length).toBe(AI_MAX_CHATS)
  expect(store.chats[0].id).toBe(`chat${AI_MAX_CHATS + 4}`)
  expect(store.chats.some(isFirstChat)).toBe(false)
})

test('a lost session starts a new one with the old messages', async function () {
  const api = new FakeApi()
  const chat = chatWith(api, new FakePrompt())
  await chat.ask(ID, 'first question', ASK)
  api.gone = ['s1']
  api.replies = [{ sessionId: 's2', message: 'Back', sql: null, isRead: false }]

  await chat.ask(ID, 'second question', ASK)

  const last = api.sent[api.sent.length - 1]
  expect(last.sessionId).toBeNull()
  expect(last.message).toContain('User: first question')
  expect(last.message).toContain('New message: second question')
  expect(chat.entries.some(isNote)).toBe(true)
})

class FakeStorage {
  items: Record<string, string> = {}

  getItem(key: string): string | null {
    return this.items[key] ?? null
  }

  setItem(key: string, value: string): void {
    this.items[key] = value
  }
}

function savedChat(id: string, updated: number): SavedChat {
  return {
    id,
    title: id,
    provider: 'claude',
    connection: 'c',
    database: 'd',
    sessionId: null,
    created: updated,
    updated,
    entries: [{ kind: 'user', text: id }],
  }
}

function isFirstChat(chat: SavedChat): boolean {
  return chat.id === 'chat0'
}

function isNote(entry: { kind: string }): boolean {
  return entry.kind === 'note'
}

test('an error of the AI tool shows in the chat', async function () {
  const broken = chatWith(new BrokenApi() as unknown as FakeApi, new FakePrompt())

  let thrown = ''
  try {
    await broken.ask(ID, 'two', { ...ASK, model: 'claude-bogus-1' })
  } catch (error) {
    thrown = String(error)
  }

  expect(thrown).toContain('claude-bogus-1')
  const last = broken.entries[broken.entries.length - 1]
  expect(last.kind).toBe('note')
  expect(last.failed).toBe(true)
  expect(last.text).toContain('selected model')
})

test('a custom model is remembered, a bad one is not', function () {
  const storage = new ProfileStorage(new FakeStorage() as unknown as Storage, '')
  const store = new AiStore(storage)
  store.model = 'claude-opus-5-5[1m]'
  store.saveSettings()
  expect(new AiStore(storage).model).toBe('claude-opus-5-5[1m]')
  expect(new AiStore(storage).knows('claude-opus-5-5[1m]')).toBe(false)

  store.model = '--dangerously-skip-permissions'
  store.saveSettings()
  expect(new AiStore(storage).model).toBe('')
})

/** An AI tool that refuses every message, like one with an unknown model. */
class BrokenApi {
  async askClaude(): Promise<ClaudeReply> {
    throw 'Claude Code: There is an issue with the selected model (claude-bogus-1).'
  }
}
