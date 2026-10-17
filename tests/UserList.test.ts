// Checks the sort and the row text of the Users view. Run: bun test
import { expect, test } from 'bun:test'
import { UserList } from '../src/services/UserList'
import type { DbUser } from '../src/types'

const LIST: DbUser[] = [
  {
    name: 'root',
    host: 'localhost',
    access: 'admin',
    current: true,
    queryStats: true,
    queryStatsFixed: true,
  },
  {
    name: 'app',
    host: '%',
    access: 'write',
    current: false,
    queryStats: false,
    queryStatsFixed: false,
  },
  {
    name: 'report',
    host: null,
    access: 'read',
    current: false,
    queryStats: true,
    queryStatsFixed: false,
  },
]

function names(users: DbUser[]): string[] {
  const result: string[] = []
  for (const user of users) {
    result.push(user.name)
  }
  return result
}

test('keeps the server order until a header is clicked', function () {
  expect(names(new UserList().visible(LIST))).toEqual(['root', 'app', 'report'])
})

test('sorts by name and turns the order around on a second click', function () {
  const list = new UserList()
  list.sortByName('Name')
  expect(list.columnName).toBe('Name')
  expect(names(list.visible(LIST))).toEqual(['app', 'report', 'root'])
  list.sortByName('Name')
  expect(names(list.visible(LIST))).toEqual(['root', 'report', 'app'])
})

test('sorts access by rights, not by label', function () {
  const list = new UserList()
  list.sortByName('Access')
  expect(names(list.visible(LIST))).toEqual(['report', 'app', 'root'])
})

test('sorts query stats with "No" first', function () {
  const list = new UserList()
  list.sortByName('Query stats')
  expect(names(list.visible(LIST))).toEqual(['app', 'root', 'report'])
  list.sortByName('Query stats')
  expect(names(list.visible(LIST))).toEqual(['root', 'report', 'app'])
})

test('puts an empty host last in both orders', function () {
  const list = new UserList()
  list.sortByName('Host')
  expect(names(list.visible(LIST))).toEqual(['app', 'root', 'report'])
  list.sortByName('Host')
  expect(names(list.visible(LIST))).toEqual(['root', 'app', 'report'])
})

test('turns a user into grid text', function () {
  expect(UserList.columns(true).length).toBe(4)
  expect(UserList.columns(false).length).toBe(3)
  expect(UserList.row(LIST[0], true)).toEqual(['root (you)', 'localhost', 'Admin', 'Yes'])
  expect(UserList.row(LIST[1], true)).toEqual(['app', '%', 'Read + Write', 'No'])
  expect(UserList.row(LIST[2], false)).toEqual(['report', 'Read', 'Yes'])
})
