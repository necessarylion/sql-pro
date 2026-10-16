import { StandardSQL, type SQLDialect } from '@codemirror/lang-sql'
import { highlightCode, tagHighlighter, tags } from '@lezer/highlight'
import MarkdownIt from 'markdown-it'
import { SQL_DIALECTS } from '../constants/sql'
import type { Driver } from '../types'

/** CSS classes for SQL parts. The colours are in AskAiView.vue, the same as the editor's. */
const SQL_CLASSES = tagHighlighter([
  { tag: tags.keyword, class: 'sql-keyword' },
  { tag: tags.string, class: 'sql-string' },
  { tag: [tags.number, tags.bool, tags.null, tags.typeName], class: 'sql-type' },
  { tag: tags.comment, class: 'sql-comment' },
])

const HTML_ESCAPES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
}

/** Collects the HTML of highlighted code. Every piece of text is escaped. */
class HtmlBuilder {
  html = ''

  text(code: string, classes: string): void {
    const safe = ChatFormatter.escape(code)
    this.html += classes ? `<span class="${classes}">${safe}</span>` : safe
  }

  lineBreak(): void {
    this.html += '\n'
  }
}

/**
 * Turns AI replies into safe HTML: Markdown, and SQL with colours.
 *
 * The text can hold data from the database, and the window can call the backend.
 * So raw HTML is never let through (`html: false`), and links and images are off:
 * a link shows as plain text and nothing is loaded or opened.
 */
export class ChatFormatter {
  private markdownIt: MarkdownIt
  private dialect: SQLDialect = StandardSQL

  constructor() {
    this.markdownIt = new MarkdownIt({
      html: false,
      linkify: false,
      typographer: false,
      highlight: this.highlight.bind(this),
    })
    this.markdownIt.disable(['link', 'image', 'autolink'])
  }

  /** An AI reply as HTML. ```sql blocks get colours in the dialect of `driver`. */
  markdown(text: string, driver?: Driver | null): string {
    this.dialect = ChatFormatter.dialectOf(driver)
    return this.markdownIt.render(text)
  }

  /** SQL as HTML with colour classes. */
  sql(code: string, driver?: Driver | null): string {
    return this.highlightSql(code, ChatFormatter.dialectOf(driver))
  }

  static escape(text: string): string {
    return text.replace(/[&<>"']/g, ChatFormatter.escapeChar)
  }

  private static escapeChar(char: string): string {
    return HTML_ESCAPES[char] ?? char
  }

  private static dialectOf(driver?: Driver | null): SQLDialect {
    return driver ? SQL_DIALECTS[driver] : StandardSQL
  }

  /** Called by markdown-it for each code block. '' lets markdown-it escape it as plain text. */
  private highlight(code: string, language: string): string {
    return language.toLowerCase() === 'sql' ? this.highlightSql(code, this.dialect) : ''
  }

  private highlightSql(code: string, dialect: SQLDialect): string {
    const builder = new HtmlBuilder()
    const tree = dialect.language.parser.parse(code)
    highlightCode(
      code,
      tree,
      SQL_CLASSES,
      builder.text.bind(builder),
      builder.lineBreak.bind(builder),
    )
    return builder.html
  }
}
