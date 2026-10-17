<script setup lang="ts">
import { onBeforeUnmount, onMounted, useTemplateRef } from 'vue'
import { LoaderCircleIcon, PlugIcon, PlusIcon, SettingsIcon, XIcon } from 'lucide-vue-next'
import ConnectionForm from './components/ConnectionForm.vue'
import DbBadge from './components/DbBadge.vue'
import EnvBadge from './components/EnvBadge.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import Workspace from './components/Workspace.vue'
import {
  LAYOUT_SERVICE,
  SESSION_MANAGER,
  SHORTCUTS,
  ZOOM_SERVICE,
  injectRequired,
} from './services/keys'
import type { Session } from './services/Session'

const manager = injectRequired(SESSION_MANAGER)
const shortcuts = injectRequired(SHORTCUTS)
const zoomService = injectRequired(ZOOM_SERVICE)
const layout = injectRequired(LAYOUT_SERVICE)
const settings = useTemplateRef<InstanceType<typeof SettingsDialog>>('settings')

/** The dim text of a tab: the open table. Without one, the database. */
function detail(session: Session): string {
  if (session.table) {
    return session.table.name
  }
  // For SQLite the database is the file path. The title already names it.
  return session.config && session.config.driver !== 'sqlite' ? session.config.database : ''
}

/**
 * The one place for the save, refresh, export, detail panel and zoom keys. Only the active tab saves, refreshes or exports.
 * The default is always stopped. It would save or reload the whole page of the app.
 */
function onKey(event: KeyboardEvent): void {
  // The zoom is for the whole app, also with no open connection.
  const zoom = shortcuts.zoom(event)
  if (zoom) {
    event.preventDefault()
    void zoomService.change(zoom)
    if (manager.active) {
      manager.active.status = `Zoom ${zoomService.percent}%`
    }
    return
  }
  // Escape or Cmd+. stops a running query. Otherwise Escape keeps its normal job.
  if (shortcuts.cancels(event) && manager.active?.cancellable) {
    event.preventDefault()
    void manager.active.cancel()
    return
  }
  const action = shortcuts.action(event)
  if (!action) {
    return
  }
  event.preventDefault()
  const session = manager.active
  // The row detail panel can open and close also while a query runs.
  if (action === 'detail') {
    if (session?.config && session.view === 'content') {
      layout.toggleDetail()
    }
    return
  }
  if (!session?.config || session.busy) {
    return
  }
  // An open cell editor or a field of the row detail panel keeps its edit as a pending change when it is left.
  const focused = document.activeElement
  if (focused instanceof HTMLTextAreaElement || focused instanceof HTMLInputElement) {
    focused.blur()
  }
  if (action === 'export') {
    // The workspace of the tab sees this and opens the export dialog.
    session.exportAsked += 1
  } else if (action === 'refresh') {
    session.run(session.refresh())
  } else if (session.view === 'content') {
    session.run(session.saveChanges())
  }
}

function listenForKeys(): void {
  window.addEventListener('keydown', onKey)
}

function stopListening(): void {
  window.removeEventListener('keydown', onKey)
}

onMounted(listenForKeys)
onBeforeUnmount(stopListening)
</script>

<template>
  <div class="app">
    <!--
      The tab bar is also the title bar. The window buttons of macOS lie over its left end.
      A drag region only works on the element that is clicked, so the tabs stay clickable.
    -->
    <div class="tabs" :class="{ mac: shortcuts.mac }" data-tauri-drag-region>
      <!-- A new tab grows in, a closed tab shrinks out, and the others slide. -->
      <TransitionGroup tag="div" name="tab" class="strip">
        <div
          v-for="session in manager.sessions"
          :key="session.id"
          class="tab"
          :class="{ active: session.id === manager.activeId, connected: !!session.config }"
          :title="detail(session) ? `${session.title} · ${detail(session)}` : session.title"
          @click="manager.activate(session)"
        >
          <DbBadge v-if="session.config" :driver="session.config.driver" />
          <PlugIcon v-else class="plug" :size="12" />
          <span class="title">{{ session.title }}</span>
          <span v-if="detail(session)" class="database">{{ detail(session) }}</span>
          <EnvBadge class="env" :name="session.environment" />
          <button
            class="icon close"
            title="Close tab"
            @click.stop="session.run(manager.close(session))"
          >
            <XIcon :size="11" />
          </button>
        </div>
      </TransitionGroup>
      <button class="icon new" title="New tab" @click="manager.open()">
        <PlusIcon :size="13" />
      </button>
      <div class="filler" data-tauri-drag-region />
      <button class="icon gear" title="Settings" @click="settings?.open()">
        <SettingsIcon :size="14" />
      </button>
    </div>

    <!-- v-show keeps the state of the hidden tabs. -->
    <div
      v-for="session in manager.sessions"
      v-show="session.id === manager.activeId"
      :key="session.id"
      class="pane"
    >
      <Workspace v-if="session.config" :session="session" />
      <div v-else-if="session.opening" class="empty">
        <LoaderCircleIcon :size="13" class="spin" />
        Connecting…
      </div>
      <ConnectionForm v-else :session="session" />
    </div>

    <SettingsDialog ref="settings" />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
}

/*
  The left padding is the room over the sidebar. Only the window buttons of macOS lie there.
  It is 8px less than the sidebar, because the strip has 8px of padding before the first tab.
  So the first tab starts exactly where the sidebar ends.
*/
.tabs {
  display: flex;
  flex: none;
  align-items: flex-end;
  height: 36px;
  padding: 0 8px 0 calc(var(--sidebar-fit) - 8px);
  background: var(--chrome);
}

/*
  The window buttons of macOS do not zoom with the page. When the page is zoomed out,
  the bar keeps its 36 real pixels, so the buttons do not hang over the area below.
*/
.tabs.mac {
  height: max(36px, calc(36px / var(--zoom)));
}

/* ponytail: very many tabs are cut at the right. Add scrolling when someone needs it. */
.strip {
  display: flex;
  flex: 0 1 auto;
  align-items: flex-end;
  min-width: 0;
  /* Room for the outer curves of the first and the last tab. */
  padding: 0 8px;
  overflow: hidden;
}

/*
  The tabs share the width. They shrink when there are many.
  The width must be a real `width`. The container query below makes the content
  count as zero wide, and the strip takes its own width from the tabs. With only
  a flex basis the strip, and so every tab, falls to the smallest width.
*/
.tab {
  position: relative;
  display: flex;
  flex: 0 1 auto;
  width: 220px;
  /* Lets the environment badge ask how wide its tab is. */
  container-type: inline-size;
  gap: 7px;
  align-items: center;
  min-width: 90px;
  height: 30px;
  padding: 0 7px 0 10px;
  border-radius: 8px 8px 0 0;
  color: var(--text-2);
  font-weight: 500;
  white-space: nowrap;
  transition:
    background-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

/* The width goes to and from zero, so the tabs next to it slide along. */
.tab-enter-active,
.tab-leave-active {
  overflow: hidden;
  transition:
    width var(--dur-med) var(--ease),
    min-width var(--dur-med) var(--ease),
    padding var(--dur-med) var(--ease),
    opacity var(--dur-med) var(--ease);
}

.tab-enter-from,
.tab-leave-to {
  width: 0;
  min-width: 0;
  padding: 0;
  opacity: 0;
}

.tab-move {
  transition: transform var(--dur-med) var(--ease);
}

.tab:not(.active):hover {
  background: color-mix(in srgb, var(--text) 8%, transparent);
}

/* A thin line between two inactive tabs. */
.tab:not(.active)::before {
  position: absolute;
  top: 8px;
  left: 0;
  width: 1px;
  height: 14px;
  background: var(--border);
  content: '';
}

/* No line at the start, next to the active tab, or next to a hovered tab. */
.tab:first-child::before,
.tab.active + .tab::before,
.tab:hover::before,
.tab:hover + .tab::before {
  display: none;
}

/* The active tab has the colour of the area below it, with no line between. */
.tab.active {
  --tab-bg: var(--bg);
  z-index: 1;
  background: var(--tab-bg);
  color: var(--text);
  font-weight: 600;
}

.tab.active.connected {
  --tab-bg: var(--toolbar);
}

/* The small outer curves at the foot of the active tab. */
.tab.active::before,
.tab.active::after {
  position: absolute;
  bottom: 0;
  width: 8px;
  height: 8px;
  content: '';
}

.tab.active::before {
  left: -8px;
  background: radial-gradient(circle at 0 0, transparent 7.5px, var(--tab-bg) 8px);
}

.tab.active::after {
  right: -8px;
  background: radial-gradient(circle at 100% 0, transparent 7.5px, var(--tab-bg) 8px);
}

.plug {
  color: var(--accent-text);
}

.title,
.database {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.title {
  flex: 1 1 auto;
}

/* The table or database name gives up its room first. */
.database {
  flex: 0 100 auto;
  color: var(--text-2);
  font-weight: 400;
}

/* A long environment name is cut. In a narrow tab the badge goes, so the name stays. */
.env {
  max-width: 72px;
}

@container (max-width: 170px) {
  .env {
    display: none;
  }
}

.close {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  color: var(--text-3);
}

.close:hover,
.new:hover,
.gear:hover {
  background: color-mix(in srgb, var(--text) 12%, transparent);
  color: var(--text);
}

.new,
.gear {
  align-self: center;
  width: 24px;
  height: 24px;
  color: var(--text-2);
}

.new {
  border-radius: 50%;
}

/* Empty room that drags the window. */
.filler {
  flex: 1 0 24px;
  align-self: stretch;
}

.pane {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  /* Fades in when its tab becomes active. */
  animation: fade-in var(--dur-fast) var(--ease);
}
</style>
