<script setup lang="ts">
import { useTemplateRef } from 'vue'
import { PlugIcon, PlusIcon, SettingsIcon, XIcon } from 'lucide-vue-next'
import ConnectionForm from './components/ConnectionForm.vue'
import DbBadge from './components/DbBadge.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import Workspace from './components/Workspace.vue'
import { SESSION_MANAGER, injectRequired } from './services/keys'

const manager = injectRequired(SESSION_MANAGER)
const settings = useTemplateRef<InstanceType<typeof SettingsDialog>>('settings')
</script>

<template>
  <div class="app">
    <!--
      The tab bar is also the title bar. The window buttons of macOS lie over its left end.
      A drag region only works on the element that is clicked, so the tabs stay clickable.
    -->
    <div class="tabs" data-tauri-drag-region>
      <div class="strip">
        <div
          v-for="session in manager.sessions"
          :key="session.id"
          class="tab"
          :class="{ active: session.id === manager.activeId, connected: !!session.config }"
          :title="session.title"
          @click="manager.activate(session)"
        >
          <DbBadge v-if="session.config" :driver="session.config.driver" />
          <PlugIcon v-else class="plug" :size="12" />
          <span class="title">{{ session.title }}</span>
          <!-- For SQLite the database is the file path. The title already names it. -->
          <span
            v-if="session.config?.database && session.config.driver !== 'sqlite'"
            class="database"
          >
            {{ session.config.database }}
          </span>
          <button
            class="icon close"
            title="Close tab"
            @click.stop="session.run(manager.close(session))"
          >
            <XIcon :size="11" />
          </button>
        </div>
      </div>
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

/* The left padding is the room for the window buttons of macOS. */
.tabs {
  display: flex;
  flex: none;
  align-items: flex-end;
  height: 36px;
  padding: 0 8px 0 78px;
  background: var(--chrome);
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

/* The tabs share the width. They shrink when there are many. */
.tab {
  position: relative;
  display: flex;
  flex: 0 1 220px;
  gap: 7px;
  align-items: center;
  min-width: 90px;
  height: 30px;
  padding: 0 7px 0 10px;
  border-radius: 8px 8px 0 0;
  color: var(--text-2);
  font-weight: 500;
  white-space: nowrap;
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

/* The database name gives up its room first. */
.database {
  flex: 0 100 auto;
  color: var(--text-2);
  font-weight: 400;
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
}
</style>
