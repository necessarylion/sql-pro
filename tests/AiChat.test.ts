// The Ask AI loop, with a fake backend. Run: bun test tests/AiChat.test.ts
import { expect, test } from 'bun:test'
import { AiChat } from '../src/services/AiChat'
import type { DatabaseApi } from '../src/services/DatabaseApi'
import type { DiscardPrompt } from '../src/services/DiscardPrompt'
import {
  AI_EDITED_NOTE,
  AI_MAX_CHATS,
  AI_MAX_CSV_FILES,
  AI_MAX_IMAGES,
  AI_MAX_STEPS,
  AI_READ_ONLY_REJECTED,
  AI_REJECTED,
  AI_RESULT_ROWS,
  AI_SESSION_GONE,
} from '../src/constants/ai'
import { AiAttachments } from '../src/services/AiAttachments'
import { AiStore } from '../src/services/AiStore'
import { ProfileStorage } from '../src/services/ProfileStorage'
import type { AiOptions, Attachment, ClaudeReply, ResultSet, SavedChat } from '../src/types'
import { FakePrompt } from './FakePrompt'

const ID = 'tab'
const ASK: AiOptions = { provider: 'claude', model: null, effort: null, readOnly: false }
const READ_ONLY: AiOptions = { ...ASK, readOnly: true }

/** Gives the planned replies one by one. Keeps what was sent to the AI and what ran. */
class FakeApi {
  replies: ClaudeReply[] = []
  sent: { message: string; sessionId: string | null }[] = []
  ran: string[] = []
  /** The SQL that ran in the read-only guard. */
  guarded: string[] = []
  /** The files of each message to the AI. */
  files: Attachment[][] = []
  result: ResultSet = { columns: ['id'], rows: [['1']], affected: 0, truncated: false }

  options: AiOptions[] = []
  /** Session ids that the AI tool has deleted. */
  gone: string[] = []

  async askClaude(
    _id: string,
    message: string,
    sessionId: string | null,
    options: AiOptions,
    files: Attachment[] = [],
  ): Promise<ClaudeReply> {
    if (sessionId && this.gone.includes(sessionId)) {
      throw `${AI_SESSION_GONE}`
    }
    this.sent.push({ message, sessionId })
    this.files.push(files)
    this.options.push(options)
    return this.replies.shift() ?? { sessionId: 's1', message: 'Done', sql: null, isRead: false }
  }

  async runAiRead(id: string, sql: string): Promise<ResultSet[]> {
    this.guarded.push(sql)
    return this.runQuery(id, sql)
  }

  async isReadSql(sql: string): Promise<boolean> {
    return /^\s*select\b/i.test(sql)
  }

  async readAttachment(path: string): Promise<Attachment> {
    const csv = path.endsWith('.csv')
    return {
      name: path,
      mediaType: csv ? 'text/csv' : 'image/png',
      data: csv ? 'email\nsecret@x.com' : 'iVBORw0KGgo=',
      rows: csv ? 2 : 0,
    }
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

/** Waits until the chat shows the approval card of a write. */
async function untilCard(chat: AiChat): Promise<void> {
  for (let tries = 0; tries < 100 && chat.phase !== 'waiting'; tries += 1) {
    await Bun.sleep(0)
  }
  expect(chat.phase).toBe('waiting')
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
  expect(api.guarded).toEqual(['SELECT id FROM users'])
  expect(api.sent[0]).toEqual({ message: 'how many users?', sessionId: null })
  expect(api.sent[1].sessionId).toBe('s1')
  expect(api.sent[1].message).toContain('Columns: id')
  expect(chat.result?.rows).toEqual([['1']])
  expect(chat.wrote).toBe(false)
  expect(chat.phase).toBe('')
})

test('a write waits in its card, then asks with the final SQL and runs on Yes', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  api.replies = [reply('UPDATE users SET blocked = 1', false)]
  api.result = { columns: [], rows: [], affected: 1, truncated: false }
  const chat = chatWith(api, prompt)

  const asked = chat.ask(ID, 'block abc@email.com', ASK)
  await untilCard(chat)
  expect(api.ran).toEqual([])
  expect(prompt.questions).toEqual([])
  chat.decide('UPDATE users SET blocked = 1')
  await asked

  expect(prompt.questions.length).toBe(1)
  expect(prompt.questions[0]).toContain('UPDATE users SET blocked = 1')
  expect(api.ran).toEqual(['UPDATE users SET blocked = 1'])
  expect(api.sent[1].message).toBe('OK. 1 rows affected.')
  expect(api.guarded).toEqual([])
  expect(chat.wrote).toBe(true)
})

test('an edited write runs, and the AI gets the edited SQL', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  api.replies = [reply('UPDATE users SET blocked = 1', false)]
  api.result = { columns: [], rows: [], affected: 1, truncated: false }
  const chat = chatWith(api, prompt)

  const asked = chat.ask(ID, 'block abc@email.com', ASK)
  await untilCard(chat)
  chat.decide('UPDATE users SET blocked = 1 WHERE id = 2')
  await asked

  expect(prompt.questions[0]).toContain('WHERE id = 2')
  expect(api.ran).toEqual(['UPDATE users SET blocked = 1 WHERE id = 2'])
  expect(api.sent[1].message).toContain(AI_EDITED_NOTE)
  expect(api.sent[1].message).toContain('UPDATE users SET blocked = 1 WHERE id = 2')
  expect(chat.entries.find(isSql)?.text).toBe('UPDATE users SET blocked = 1 WHERE id = 2')
})

test('a write edited into a read runs in the guard without a dialog', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  api.replies = [reply('DELETE FROM users', false)]
  const chat = chatWith(api, prompt)

  const asked = chat.ask(ID, 'clean up', ASK)
  await untilCard(chat)
  chat.decide('SELECT count(*) FROM users')
  await asked

  expect(prompt.questions).toEqual([])
  expect(api.guarded).toEqual(['SELECT count(*) FROM users'])
  expect(chat.wrote).toBe(false)
})

test('No in the dialog after Run still rejects', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  prompt.answer = false
  api.replies = [reply('DELETE FROM users', false)]
  const chat = chatWith(api, prompt)

  const asked = chat.ask(ID, 'delete everyone', ASK)
  await untilCard(chat)
  chat.decide('DELETE FROM users')
  await asked

  expect(api.ran).toEqual([])
  expect(api.sent[1].message).toBe(AI_REJECTED)
})

test('a rejected write does not run and the AI is told', async function () {
  const api = new FakeApi()
  const prompt = new FakePrompt()
  api.replies = [reply('DELETE FROM users', false)]
  const chat = chatWith(api, prompt)

  const asked = chat.ask(ID, 'delete everyone', ASK)
  await untilCard(chat)
  chat.decide(null)
  await asked

  expect(prompt.questions).toEqual([])
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

test('files can be added and removed, and the limits hold', function () {
  const files = new AiAttachments()
  const image: Attachment = { name: 'a.png', mediaType: 'image/png', data: 'x', rows: 0 }
  const csv: Attachment = { name: 'u.csv', mediaType: 'text/csv', data: 'a', rows: 1 }
  for (let index = 0; index < AI_MAX_IMAGES; index += 1) {
    files.add(image)
  }
  expect(throws(files, image)).toContain(`${AI_MAX_IMAGES} images`)
  for (let index = 0; index < AI_MAX_CSV_FILES; index += 1) {
    files.add(csv)
  }
  expect(throws(files, csv)).toContain(`${AI_MAX_CSV_FILES} CSV`)
  expect(files.items.length).toBe(AI_MAX_IMAGES + AI_MAX_CSV_FILES)
  files.remove(0)
  expect(files.items.length).toBe(AI_MAX_IMAGES + AI_MAX_CSV_FILES - 1)
  files.add(image)
  expect(files.take().length).toBe(AI_MAX_IMAGES + AI_MAX_CSV_FILES)
  expect(files.items).toEqual([])
  expect(AiAttachments.toBase64(new Uint8Array([104, 105]))).toBe('aGk=')
})

test('files go with the question only, and the history keeps no content', async function () {
  const api = new FakeApi()
  api.replies = [reply('SELECT 1', true)]
  const chat = chatWith(api, new FakePrompt())
  await chat.attachFile('users.csv')
  await chat.attachFile('shot.png')

  await chat.ask(ID, 'add these users', ASK)

  expect(api.files[0].length).toBe(2)
  expect(api.files[1]).toEqual([])
  expect(chat.attachments.items).toEqual([])
  // The live chat keeps the data in memory for the preview. The saved chat does not.
  expect(chat.entries[0].files?.[0]).toEqual({
    name: 'users.csv',
    mediaType: 'text/csv',
    rows: 2,
    kept: true,
    data: 'email\nsecret@x.com',
  })
  const saved = JSON.stringify(chat.snapshot('c', 'd'))
  expect(saved).not.toContain('secret@x.com')
  expect(saved).not.toContain('iVBORw0KGgo=')
  expect(saved).toContain('"kept":false')
})

test('a production connection asks once, and a Yes is remembered', async function () {
  const storage = new ProfileStorage(new FakeStorage() as unknown as Storage, '')
  const asker = new FakeAsker()
  const store = new AiStore(storage)

  expect(await store.allowSend('dev-db', 'dev', asker)).toBe(true)
  expect(asker.questions).toBe(0)

  asker.answer = false
  expect(await store.allowSend('prod-db', 'prod', asker)).toBe(false)
  asker.answer = true
  expect(await store.allowSend('prod-db', 'prod', asker)).toBe(true)
  expect(asker.questions).toBe(2)

  expect(await new AiStore(storage).allowSend('prod-db', 'prod', asker)).toBe(true)
  expect(asker.questions).toBe(2)
  expect(await store.allowSend('other-prod', 'prod', asker)).toBe(true)
  expect(asker.questions).toBe(3)
})

class FakeAsker {
  answer = true
  questions = 0

  async confirm(): Promise<boolean> {
    this.questions += 1
    return this.answer
  }
}

function throws(files: AiAttachments, file: Attachment): string {
  try {
    files.add(file)
  } catch (error) {
    return String(error)
  }
  return ''
}

test('CSV rows become a grid, with short and long rows', function () {
  const table = AiAttachments.csvTable([
    ['email', 'name'],
    ['a@x.com'],
    ['b@x.com', 'Bob', 'extra'],
  ])

  expect(table.columns).toEqual(['email', 'name', 'Column 3'])
  expect(table.rows).toEqual([
    ['a@x.com', '', ''],
    ['b@x.com', 'Bob', 'extra'],
  ])
  expect(AiAttachments.csvTable([])).toEqual({ columns: [], rows: [] })
})
