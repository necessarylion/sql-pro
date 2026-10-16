import {
  DATE_TYPE_PATTERN,
  DATETIME_TYPE_PATTERN,
  DETAIL_LINE_LENGTH,
  JSON_INDENT,
  ROW_VERSION_TYPE_PATTERN,
  TIME_TYPE_PATTERN,
  ZONED_TIME_TYPE_PATTERN,
} from '../constants'
import type { Driver } from '../types'

/** The `type` of the native `<input>` that picks a value. */
export type PickerType = 'date' | 'time' | 'datetime-local'

/** The date part and the time part of a picker value. '' = that part is empty or not used. */
export interface PickerParts {
  date: string
  time: string
}

/** A time as the database writes it. At most 3 digits of a fraction are kept, the input takes no more. */
const TIME = String.raw`(\d{2}:\d{2}(?::\d{2}(?:\.\d{1,3})?)?)\d*`
const PICKER_TEXT: Record<PickerType, RegExp> = {
  date: /^(\d{4}-\d{2}-\d{2})$/,
  time: new RegExp(`^${TIME}$`),
  'datetime-local': new RegExp(`^(\\d{4}-\\d{2}-\\d{2})[ T]${TIME}$`),
}
/** A time without seconds at the end. The input leaves out ":00" seconds. */
const NO_SECONDS = /(^|\s)\d{2}:\d{2}$/

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

  /**
   * The native picker for a column type. null = no picker.
   * Types with a time zone get none: the picker would lose the zone.
   */
  static pickerType(type: string, driver?: Driver): PickerType | null {
    const text = type.trim()
    if (ZONED_TIME_TYPE_PATTERN.test(text)) {
      return null
    }
    if (driver === 'mssql' && ROW_VERSION_TYPE_PATTERN.test(text)) {
      return null
    }
    if (DATE_TYPE_PATTERN.test(text)) {
      return 'date'
    }
    if (TIME_TYPE_PATTERN.test(text)) {
      return 'time'
    }
    return DATETIME_TYPE_PATTERN.test(text) ? 'datetime-local' : null
  }

  /** The database text as a value of the picker. '' when it cannot be read, so the picker opens empty. */
  static toPickerValue(value: string | null, picker: PickerType): string {
    const match = PICKER_TEXT[picker].exec(value?.trim() ?? '')
    if (!match) {
      return ''
    }
    return picker === 'datetime-local' ? `${match[1]}T${match[2]}` : (match[1] ?? '')
  }

  /** A value of the picker as database text: a space, not "T", and always with seconds. */
  static fromPickerValue(value: string): string {
    const text = value.replace('T', ' ')
    return NO_SECONDS.test(text) ? `${text}:00` : text
  }

  /** The database text split into the date and the time input of the popover. */
  static toPickerParts(value: string | null, picker: PickerType): PickerParts {
    const text = FieldValue.toPickerValue(value, picker)
    if (picker === 'date') {
      return { date: text, time: '' }
    }
    if (picker === 'time') {
      return { date: '', time: text }
    }
    const [date = '', time = ''] = text.split('T')
    return { date, time }
  }

  /**
   * The date and time inputs as database text. null when a needed part is empty.
   * A date and time column with only a date gets midnight.
   */
  static fromPickerParts(parts: PickerParts, picker: PickerType): string | null {
    if (picker === 'date') {
      return parts.date === '' ? null : parts.date
    }
    if (picker === 'time') {
      return parts.time === '' ? null : FieldValue.fromPickerValue(parts.time)
    }
    if (parts.date === '') {
      return null
    }
    return FieldValue.fromPickerValue(
      `${parts.date}T${parts.time === '' ? '00:00:00' : parts.time}`,
    )
  }

  /** A local date and time as picker parts, like 2024-01-02 and 03:04:05. */
  static nowParts(now: Date): PickerParts {
    const two = FieldValue.twoDigits
    return {
      date: `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`,
      time: `${two(now.getHours())}:${two(now.getMinutes())}:${two(now.getSeconds())}`,
    }
  }

  private static twoDigits(value: number): string {
    return String(value).padStart(2, '0')
  }
}
