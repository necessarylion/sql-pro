// The AI text becomes HTML. Nothing unsafe may come through. Run: bun test tests/ChatFormatter.test.ts
import { expect, test } from 'bun:test'
import { ChatFormatter } from '../src/services/ChatFormatter'

const formatter = new ChatFormatter()

test('markdown renders lists, bold and tables', function () {
  const html = formatter.markdown(
    '**Bold** and *it*\n\n1. one\n2. two\n\n| a | b |\n|---|---|\n| 1 | 2 |',
  )
  expect(html).toContain('<strong>Bold</strong>')
  expect(html).toContain('<em>it</em>')
  expect(html).toContain('<ol>')
  expect(html).toContain('<table>')
})

test('raw HTML, images and javascript links do not come through', function () {
  const html = formatter.markdown(
    [
      '<script>alert(1)</script>',
      '<img src=x onerror="alert(1)">',
      '[click](javascript:alert(1))',
      '![pic](https://example.com/a.png)',
      '<https://example.com>',
      "```sql\nSELECT '<img src=x onerror=alert(1)>'\n```",
    ].join('\n\n'),
    'mysql',
  )
  expect(html).not.toContain('<script')
  expect(html).not.toContain('<img')
  expect(html).not.toContain('<a ')
  expect(html).not.toContain('href')
  expect(html).not.toMatch(/<[^>]*onerror/)
  expect(html).toContain('&lt;script&gt;')
})

test('SQL gets colour classes and stays escaped', function () {
  const html = formatter.sql("SELECT 'a<b' FROM t -- note", 'postgres')
  expect(html).toContain('<span class="sql-keyword">SELECT</span>')
  expect(html).toContain('sql-string')
  expect(html).toContain('a&lt;b')
  expect(html).toContain('sql-comment')

  const block = formatter.markdown('```sql\nSELECT 1\n```')
  expect(block).toContain('<span class="sql-keyword">SELECT</span>')
})
