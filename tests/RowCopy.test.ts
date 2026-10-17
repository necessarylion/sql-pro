// Checks the tab-separated text of copied rows. Run: bun test
import { expect, test } from 'bun:test'
import { RowCopy } from '../src/services/RowCopy'

const copy = new RowCopy()

test('rows become lines and cells are joined by a tab', function () {
  const rows = [
    ['1', 'Ann'],
    ['2', 'Bob'],
  ]
  expect(copy.text(rows, [0, 1])).toBe('1\tAnn\n2\tBob')
})

test('NULL is written as NULL and an empty value stays empty', function () {
  expect(copy.text([['1', null, '']], [0, 1, 2])).toBe('1\tNULL\t')
})

test('tabs, line breaks and quotes are quoted', function () {
  expect(copy.cell('a\tb')).toBe('"a\tb"')
  expect(copy.cell('a\nb')).toBe('"a\nb"')
  expect(copy.cell('a\r\nb')).toBe('"a\r\nb"')
  expect(copy.cell('say "hi"')).toBe('"say ""hi"""')
  expect(copy.cell('plain')).toBe('plain')
})

test('the columns come in the shown order', function () {
  expect(copy.text([['1', 'Ann', 'x']], [2, 0, 1])).toBe('x\t1\tAnn')
})

// "Copy Rows As": one check per format.
const PICK = {
  columns: ['id', 'note'],
  rows: [
    ['1', 'a|b, "c" <d> & e'],
    ['2', null],
  ],
}

test('plain text is the tab-separated text', function () {
  expect(copy.as('text', PICK)).toBe('1\t"a|b, ""c"" <d> & e"\n2\tNULL')
})

test('JSON is an array of objects with null, indented by two spaces, in column order', function () {
  const text = copy.as('json', { columns: ['b', '1'], rows: [['x', null]] })
  expect(text).toBe('[\n  {\n    "b": "x",\n    "1": null\n  }\n]')
  expect(JSON.parse(copy.as('json', PICK))).toEqual([
    { id: '1', note: 'a|b, "c" <d> & e' },
    { id: '2', note: null },
  ])
  expect(copy.as('json', { columns: ['id'], rows: [] })).toBe('[]')
})

test('HTML has a head row, escapes text and leaves NULL empty', function () {
  expect(copy.as('html', PICK)).toBe(
    [
      '<table>',
      '  <thead>',
      '    <tr><th>id</th><th>note</th></tr>',
      '  </thead>',
      '  <tbody>',
      '    <tr><td>1</td><td>a|b, &quot;c&quot; &lt;d&gt; &amp; e</td></tr>',
      '    <tr><td>2</td><td></td></tr>',
      '  </tbody>',
      '</table>',
    ].join('\n'),
  )
})

test('Markdown escapes the pipe and turns line breaks into <br>', function () {
  const pick = {
    columns: ['id', 'a|b'],
    rows: [
      ['1', 'x|y\r\nz\nw'],
      ['2', null],
    ],
  }
  expect(copy.as('markdown', pick)).toBe(
    '| id | a\\|b |\n| --- | --- |\n| 1 | x\\|y<br>z<br>w |\n| 2 | NULL |',
  )
})

test('CSV quotes only what needs it, and NULL is an empty field', function () {
  const pick = {
    columns: ['id', 'note'],
    rows: [
      ['1', 'a,b'],
      ['2', 'say "hi"'],
      ['3', 'x\ny'],
      ['4', null],
    ],
  }
  const body = '1,"a,b"\r\n2,"say ""hi"""\r\n3,"x\ny"\r\n4,'
  expect(copy.as('csv', pick)).toBe(body)
  expect(copy.as('csvHeader', pick)).toBe(`id,note\r\n${body}`)
})
