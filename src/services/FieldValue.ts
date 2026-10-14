import { DETAIL_LINE_LENGTH, JSON_INDENT } from '../constants'

/** Decides how the row detail panel shows one value. */
export class FieldValue {
  /** True when the value needs a multi-line box: it is long or has a line break. NULL is short. */
  static multiline(value: string | null): boolean {
    return value !== null && (value.length > DETAIL_LINE_LENGTH || value.includes('\n'))
  }

  /**
   * The value as pretty printed JSON, when it is a JSON object or array.
   * null when it is not JSON, or is only a number, string or boolean.
   */
  static prettyJson(value: string | null): string | null {
    const text = value?.trim() ?? ''
    if (!text.startsWith('{') && !text.startsWith('[')) {
      return null
    }
    try {
      return JSON.stringify(JSON.parse(text), null, JSON_INDENT)
    } catch {
      return null
    }
  }
}
