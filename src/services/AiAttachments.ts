import { AI_MAX_CSV_FILES, AI_MAX_IMAGES } from '../constants/ai'
import type { Attachment } from '../types'

/** The files that go with the next message. The backend checks each file and the counts again. */
export class AiAttachments {
  items: Attachment[] = []

  /** Adds a checked file. Throws the reason as text when the message has too many. */
  add(file: Attachment): void {
    const csv = AiAttachments.isCsv(file)
    let same = 0
    for (const item of this.items) {
      if (AiAttachments.isCsv(item) === csv) {
        same += 1
      }
    }
    if (csv && same >= AI_MAX_CSV_FILES) {
      throw `At most ${AI_MAX_CSV_FILES} CSV files can go with one message.`
    }
    if (!csv && same >= AI_MAX_IMAGES) {
      throw `At most ${AI_MAX_IMAGES} images can go with one message.`
    }
    this.items = [...this.items, file]
  }

  remove(index: number): void {
    this.items = [...this.items.slice(0, index), ...this.items.slice(index + 1)]
  }

  /** Gives all files and empties the list. */
  take(): Attachment[] {
    const items = this.items
    this.items = []
    return items
  }

  /** The bytes as base64, for a pasted image. */
  static toBase64(bytes: Uint8Array): string {
    let binary = ''
    // In parts: one huge `fromCharCode` call would be too big for the stack.
    for (let start = 0; start < bytes.length; start += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(start, start + 0x8000))
    }
    return btoa(binary)
  }

  /** A small picture of an image for its chip. */
  static preview(file: { mediaType: string; data?: string }): string {
    return `data:${file.mediaType};base64,${file.data ?? ''}`
  }

  static isCsv(file: { mediaType: string }): boolean {
    return file.mediaType === 'text/csv'
  }
}
