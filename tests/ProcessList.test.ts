// Checks the sort and the row text of the Processes view. Run: bun test
import { expect, test } from 'bun:test'
import { ProcessList } from '../src/services/ProcessList'
import type { DbProcess } from '../src/types'

const BASE: DbProcess = {
  id: 9,
  user: 'root',
  host: 'localhost',
  database: 'shop',
  state: 'Query',
  seconds: 5,
  query: 'SELECT 1',
  current: false,
}
const LIST: DbProcess[] = [
  BASE,
  { ...BASE, id: 10, seconds: null, user: null },
  { ...BASE, id: 100, seconds: 70, user: 'app' },
]

function ids(processes: DbProcess[]): number[] {
  const result: number[] = []
  for (const process of processes) {
    result.push(process.id)
  }
  return result
}

test('keeps the server order until a header is clicked', function () {
  expect(ids(new ProcessList().visible(LIST))).toEqual([9, 10, 100])
})

test('sorts numbers by value, not as text', function () {
  const list = new ProcessList()
  list.sortByName('Id')
  expect(list.columnName).toBe('Id')
  expect(list.descending).toBe(true)
  expect(ids(list.visible(LIST))).toEqual([100, 10, 9])
  list.sortByName('Id')
  expect(ids(list.visible(LIST))).toEqual([9, 10, 100])
})

test('puts empty values last in both orders', function () {
  const list = new ProcessList()
  list.sortByName('Time')
  expect(ids(list.visible(LIST))).toEqual([100, 9, 10])
  list.sortByName('Time')
  expect(ids(list.visible(LIST))).toEqual([9, 100, 10])
  list.sortByName('User')
  expect(list.descending).toBe(false)
  expect(ids(list.visible(LIST))).toEqual([100, 9, 10])
})

test('turns a process into grid text', function () {
  expect(ProcessList.row(LIST[2])).toEqual([
    '100',
    'app',
    'localhost',
    'shop',
    'Query',
    '1m 10s',
    'SELECT 1',
    '',
  ])
  const own = ProcessList.row({ ...LIST[1], current: true })
  expect(own[0]).toBe('10')
  expect(own[7]).toBe('this tab')
  expect(ProcessList.row(LIST[1])[5]).toBeNull()
  expect(ProcessList.duration(7300)).toBe('2h 1m')
})

test('sorts by the Current mark, with the empty ones last', function () {
  const list = new ProcessList()
  list.sortByName('Current')
  const own: DbProcess[] = [BASE, { ...BASE, id: 10, current: true }]
  expect(ids(list.visible(own))).toEqual([10, 9])
  list.sortByName('Current')
  expect(ids(list.visible(own))).toEqual([10, 9])
})

test('gives number columns a numeric type', function () {
  const columns = ProcessList.columns()
  expect(columns[0]).toEqual({ name: 'Id', type: 'numeric' })
  expect(columns[1].type).toBeUndefined()
  expect(columns[5].type).toBe('numeric')
  expect(columns[7]).toEqual({ name: 'Current', type: undefined })
})
