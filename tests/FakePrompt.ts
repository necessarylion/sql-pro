/** Stands in for the questions of the Session. It keeps the questions and gives `answer`. */
export class FakePrompt {
  answer = true
  questions: string[] = []

  async ask(message: string): Promise<boolean> {
    this.questions.push(message)
    return this.answer
  }
}
