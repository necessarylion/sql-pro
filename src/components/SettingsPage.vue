<script setup lang="ts">
import { onBeforeUnmount, onMounted, useTemplateRef } from 'vue'
import { FONT_SIZE_MAX, FONT_SIZE_MIN } from '../constants/fonts'
import { THEME_LABELS, type ThemeChoice } from '../constants/theme'
import { FONT_SERVICE, SETTINGS_WINDOW, THEME_SERVICE, injectRequired } from '../services/keys'

// The page of the Settings window. Each change is saved, then sent to the main window.
const theme = injectRequired(THEME_SERVICE)
const fonts = injectRequired(FONT_SERVICE)
const settingsWindow = injectRequired(SETTINGS_WINDOW)
const content = useTemplateRef<HTMLElement>('content')

function publish(): void {
  settingsWindow.publish().catch(console.error)
}

function chooseTheme(choice: ThemeChoice): void {
  theme.choose(choice)
  publish()
}

function pickedName(event: Event): string {
  return (event.target as HTMLSelectElement).value
}

function chooseUiFont(event: Event): void {
  fonts.chooseUiFont(pickedName(event))
  publish()
}

function chooseMonoFont(event: Event): void {
  fonts.chooseMonoFont(pickedName(event))
  publish()
}

/** A size out of range is moved into range. The field then shows the size that is used. */
function chooseSize(event: Event): void {
  const input = event.target as HTMLInputElement
  fonts.chooseSize(input.valueAsNumber)
  input.value = String(fonts.size)
  publish()
}

/**
 * Escape and Cmd+W / Ctrl+W close the window. On macOS the app menu also closes it with Cmd+W.
 * Windows and Linux have no menu, so the page does it.
 */
function onKey(event: KeyboardEvent): void {
  const closeKey = (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'w'
  if (event.key === 'Escape' || closeKey) {
    event.preventDefault()
    settingsWindow.close().catch(console.error)
  }
}

function start(): void {
  window.addEventListener('keydown', onKey)
  // The window starts hidden. It gets the height of the content, then it shows.
  if (content.value) {
    settingsWindow.reveal(content.value).catch(console.error)
  }
  // The list is only needed here. Without it, the pickers still show the saved fonts.
  void fonts.loadFonts().catch(console.error)
}

function stop(): void {
  window.removeEventListener('keydown', onKey)
}

onMounted(start)
onBeforeUnmount(stop)
</script>

<template>
  <main class="settings">
    <div ref="content" class="content">
      <section>
        <h2>Appearance</h2>
        <div class="group">
          <div class="row">
            <span>Theme</span>
            <div class="segmented">
              <button
                v-for="(text, choice) in THEME_LABELS"
                :key="choice"
                :class="{ active: theme.choice === choice }"
                @click="chooseTheme(choice)"
              >
                {{ text }}
              </button>
            </div>
          </div>
        </div>
      </section>
      <section>
        <h2>Fonts</h2>
        <div class="group">
          <label class="row">
            <span>Font</span>
            <select :value="fonts.uiFont" @change="chooseUiFont">
              <option value="">Default</option>
              <option
                v-if="fonts.uiFont && !fonts.fonts.includes(fonts.uiFont)"
                :value="fonts.uiFont"
              >
                {{ fonts.uiFont }}
              </option>
              <option v-for="name in fonts.fonts" :key="name" :value="name">{{ name }}</option>
            </select>
          </label>
          <label class="row">
            <span>Code font</span>
            <select :value="fonts.monoFont" @change="chooseMonoFont">
              <option value="">Default</option>
              <option
                v-if="fonts.monoFont && !fonts.fonts.includes(fonts.monoFont)"
                :value="fonts.monoFont"
              >
                {{ fonts.monoFont }}
              </option>
              <option v-for="name in fonts.fonts" :key="name" :value="name">{{ name }}</option>
            </select>
          </label>
          <label class="row">
            <span>Font size</span>
            <input
              type="number"
              :min="FONT_SIZE_MIN"
              :max="FONT_SIZE_MAX"
              :value="fonts.size"
              @change="chooseSize"
            />
          </label>
        </div>
      </section>
    </div>
  </main>
</template>

<style scoped>
/* Only this window loads this page, so the body colour changes only here. */
:global(body) {
  background: var(--toolbar);
}

/* The page fills the window. Only above the largest window height it scrolls. */
.settings {
  height: 100vh;
  overflow-y: auto;
  background: var(--toolbar);
  color: var(--text);
}

/*
  The window measures this box and fits to it. Its height never depends on the
  window: no height limit, and it does not shrink. The padding counts in its height.
*/
.content {
  display: flex;
  flex-direction: column;
  gap: 14px;
  box-sizing: border-box;
  padding: 14px 16px 16px;
}

.content > section {
  flex: none;
}

section {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

h2 {
  margin: 0 0 0 2px;
  color: var(--text-2);
  font-size: calc(11px * var(--font-scale));
  font-weight: 600;
}

/* Like a grouped box of macOS System Settings: rows with thin lines between them. */
.group {
  background: var(--bg);
  border: 1px solid var(--border-soft);
  border-radius: 8px;
}

/* The label sits on the left, the control on the right. */
.row {
  display: flex;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  min-height: 34px;
  padding: 0 10px;
}

.row + .row {
  border-top: 1px solid var(--border-soft);
}

select {
  width: 190px;
}

input[type='number'] {
  width: 56px;
}

.segmented button {
  height: 22px;
  border-radius: 5px;
}
</style>
