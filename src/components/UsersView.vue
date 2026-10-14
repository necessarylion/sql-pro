<script setup lang="ts">
import { computed, onMounted, ref, useTemplateRef } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { EyeIcon, EyeOffIcon, KeyRoundIcon, Trash2Icon, UsersIcon } from 'lucide-vue-next'
import {
  ACCESS_LABELS,
  ACCESS_ORDER,
  DEFAULT_USER_HOST,
  GRANTABLE_ACCESS,
} from '../constants/users'
import type { Session } from '../services/Session'
import type { DbUser, GrantableAccess } from '../types'
import BottomBar from './BottomBar.vue'

const props = defineProps<{ session: Session }>()
const hasHosts = computed(usesHosts)
const count = computed(describeCount)
const note = computed(describeAccess)

// The form for a new user.
const newName = ref('')
const newHost = ref(DEFAULT_USER_HOST)
const newPassword = ref('')
const newAccess = ref<GrantableAccess>('read')
const showPassword = ref(false)

// The dialog that changes the password of one user.
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const passwordUser = ref<DbUser | null>(null)
const changedPassword = ref('')
const repeatedPassword = ref('')
const showChanged = ref(false)
const mismatch = computed(passwordsDiffer)

// A query may have changed users or rights. So load the list each time the view opens.
onMounted(reload)

function reload(): void {
  props.session.loadUsers()
}

/** Only MySQL accounts have a host. */
function usesHosts(): boolean {
  return props.session.config?.driver === 'mysql'
}

function describeCount(): string {
  const users = props.session.users.length
  return `${users} ${users === 1 ? 'user' : 'users'}`
}

/** Says what the Access column means. */
function describeAccess(): string {
  const database = props.session.config?.database
  return database ? `Access on database "${database}"` : 'Choose a database to see access'
}

/** Why the access of this user cannot be changed here. '' when it can. */
function lockReason(user: DbUser): string {
  if (user.current) {
    return 'You cannot change your own access'
  }
  return user.access === 'admin' ? 'Admin access cannot be changed here' : ''
}

function changeAccess(user: DbUser, event: Event): void {
  const select = event.target as HTMLSelectElement
  const access = select.value as GrantableAccess
  // Show the old level again. The new one shows when the list is loaded again.
  select.value = user.access
  props.session.run(sendAccess(user, access))
}

/** Asks first. Changes the access only on yes. */
async function sendAccess(user: DbUser, access: GrantableAccess): Promise<void> {
  const database = props.session.config?.database
  if (!database) {
    throw 'Choose a database first'
  }
  let question = `Change access of "${user.name}" on database "${database}" from ${ACCESS_LABELS[user.access]} to ${ACCESS_LABELS[access]}?`
  if (ACCESS_ORDER.indexOf(access) < ACCESS_ORDER.indexOf(user.access)) {
    question += ' The user loses rights at once.'
  }
  if (await confirm(question, { title: 'Change access', kind: 'warning' })) {
    await props.session.setUserAccess(user, access)
  }
}

/** Asks first. Removes the user only on yes. */
async function remove(user: DbUser): Promise<void> {
  const host = user.host === null ? '' : `@"${user.host}"`
  const sure = await confirm(
    `Remove user "${user.name}"${host}? The account is deleted and cannot log in any more. This cannot be undone.`,
    { title: 'Remove user', kind: 'warning' },
  )
  if (sure) {
    await props.session.dropUser(user)
  }
}

/** The native dialog closes on Escape by itself. */
function openPasswordDialog(user: DbUser): void {
  passwordUser.value = user
  dialog.value?.showModal()
}

function closePasswordDialog(): void {
  dialog.value?.close()
}

/** The dialog has no padding. A click on the dialog itself is a click outside the panel. */
function closeOnOutsideClick(event: MouseEvent): void {
  if (event.target === dialog.value) {
    closePasswordDialog()
  }
}

/** Runs on every close, also on Escape. So a password never stays in the fields. */
function clearPasswordForm(): void {
  changedPassword.value = ''
  repeatedPassword.value = ''
  showChanged.value = false
}

/** True when both fields are filled and not the same. */
function passwordsDiffer(): boolean {
  return (
    changedPassword.value !== '' &&
    repeatedPassword.value !== '' &&
    changedPassword.value !== repeatedPassword.value
  )
}

/** Closes and clears the dialog first. So the fields are empty on success and on failure. */
function changePassword(): void {
  const user = passwordUser.value
  const password = changedPassword.value
  if (!user || !password || password !== repeatedPassword.value) {
    return
  }
  clearPasswordForm()
  closePasswordDialog()
  props.session.run(props.session.setUserPassword(user, password))
}

function addUser(): void {
  if (newName.value.trim() && newPassword.value) {
    props.session.run(sendNewUser())
  }
}

/** The password field is always cleared. The rest of the form only when the user was added. */
async function sendNewUser(): Promise<void> {
  const password = newPassword.value
  newPassword.value = ''
  showPassword.value = false
  await props.session.createUser({
    name: newName.value.trim(),
    host: hasHosts.value ? newHost.value.trim() || DEFAULT_USER_HOST : null,
    password,
    access: newAccess.value,
  })
  newName.value = ''
  newHost.value = DEFAULT_USER_HOST
  newAccess.value = 'read'
}
</script>

<template>
  <div class="bar info">
    <UsersIcon class="glyph" :size="13" />
    <span class="name">Users</span>
    <span class="dim">{{ count }}</span>
    <span class="dim">{{ note }}</span>
  </div>
  <div class="grid">
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th v-if="hasHosts">Host</th>
          <th>Access</th>
          <th />
        </tr>
      </thead>
      <tbody>
        <tr v-for="user in session.users" :key="`${user.name}@${user.host}`">
          <td>
            {{ user.name }}
            <span v-if="user.current" class="pill">you</span>
          </td>
          <td v-if="hasHosts">{{ user.host }}</td>
          <td>
            <span v-if="lockReason(user)" class="dim" :title="lockReason(user)">
              {{ ACCESS_LABELS[user.access] }}
            </span>
            <select
              v-else
              :value="user.access"
              :title="`Access of ${user.name}`"
              @change="changeAccess(user, $event)"
            >
              <option v-if="user.access === 'custom'" value="custom" disabled>
                {{ ACCESS_LABELS.custom }}
              </option>
              <option v-for="level in GRANTABLE_ACCESS" :key="level" :value="level">
                {{ ACCESS_LABELS[level] }}
              </option>
            </select>
          </td>
          <td>
            <button class="icon" title="Change password" @click="openPasswordDialog(user)">
              <KeyRoundIcon :size="12" />
            </button>
            <button
              class="icon"
              :title="user.current ? 'You cannot remove your own account' : 'Remove this user'"
              :disabled="user.current"
              @click="session.run(remove(user))"
            >
              <Trash2Icon :size="12" />
            </button>
          </td>
        </tr>
      </tbody>
    </table>
  </div>

  <BottomBar :session="session">
    <input
      v-model="newName"
      class="new-name mono"
      placeholder="new user name"
      autocapitalize="off"
      spellcheck="false"
      @keydown.enter="addUser"
    />
    <input
      v-if="hasHosts"
      v-model="newHost"
      class="new-host mono"
      :placeholder="DEFAULT_USER_HOST"
      title="The host the user may log in from. % means any host."
      autocapitalize="off"
      spellcheck="false"
      @keydown.enter="addUser"
    />
    <input
      v-model="newPassword"
      class="new-password mono"
      :type="showPassword ? 'text' : 'password'"
      placeholder="password"
      autocapitalize="off"
      autocomplete="new-password"
      spellcheck="false"
      @keydown.enter="addUser"
    />
    <button
      class="icon"
      :title="showPassword ? 'Hide the password' : 'Show the password'"
      @click="showPassword = !showPassword"
    >
      <component :is="showPassword ? EyeOffIcon : EyeIcon" :size="13" />
    </button>
    <select v-model="newAccess" title="Access on this database">
      <option v-for="level in GRANTABLE_ACCESS" :key="level" :value="level">
        {{ ACCESS_LABELS[level] }}
      </option>
    </select>
    <button class="primary" :disabled="!newName.trim() || !newPassword" @click="addUser">
      Add user
    </button>
  </BottomBar>

  <dialog ref="dialog" class="password" @click="closeOnOutsideClick" @close="clearPasswordForm">
    <div class="panel">
      <h2>Change password of "{{ passwordUser?.name }}"</h2>
      <p v-if="passwordUser?.current" class="dim">
        This is your own account. Saved favorites for this connection still hold the old password.
      </p>
      <div class="field">
        <input
          v-model="changedPassword"
          :type="showChanged ? 'text' : 'password'"
          placeholder="New password"
          autocapitalize="off"
          autocomplete="new-password"
          spellcheck="false"
          @keydown.enter="changePassword"
        />
        <button
          class="icon"
          :title="showChanged ? 'Hide the passwords' : 'Show the passwords'"
          @click="showChanged = !showChanged"
        >
          <component :is="showChanged ? EyeOffIcon : EyeIcon" :size="13" />
        </button>
      </div>
      <div class="field">
        <input
          v-model="repeatedPassword"
          :type="showChanged ? 'text' : 'password'"
          placeholder="Repeat password"
          autocapitalize="off"
          autocomplete="new-password"
          spellcheck="false"
          @keydown.enter="changePassword"
        />
      </div>
      <div class="actions">
        <span v-if="mismatch" class="error">The passwords do not match</span>
        <span class="spacer" />
        <button @click="closePasswordDialog">Cancel</button>
        <button
          class="primary"
          :disabled="!changedPassword || changedPassword !== repeatedPassword"
          @click="changePassword"
        >
          Change password
        </button>
      </div>
    </div>
  </dialog>
</template>

<style scoped>
.info {
  gap: 10px;
  padding: 0 10px;
}

.glyph {
  color: var(--accent-text);
}

.name {
  font-size: 12px;
  font-weight: 600;
}

/* The same look as the DataGrid. A small table of its own, because rows hold controls. */
.grid {
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--bg);
  font-size: 11px;
}

table {
  border-spacing: 0;
}

th,
td {
  height: 22px;
  padding: 0 8px;
  text-align: left;
  white-space: nowrap;
}

th {
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--header);
  border-right: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  color: var(--text-2);
  font-weight: 600;
}

td {
  border-right: 1px solid var(--border-soft);
  font-family: var(--font-mono);
  -webkit-user-select: text;
  user-select: text;
}

tbody tr:last-child td {
  border-bottom: 1px solid var(--border-soft);
}

tbody tr:nth-child(even) {
  background: var(--row-alt);
}

td .pill {
  height: 14px;
  margin-left: 4px;
  padding: 0 6px;
  font-family: var(--font-ui);
  font-size: 10px;
}

td .dim,
select {
  font-family: var(--font-ui);
  font-size: 11px;
}

select {
  height: 18px;
  padding: 0 4px;
  border-radius: 4px;
}

td button.icon {
  height: 18px;
}

.bar select {
  height: 22px;
  border-radius: 5px;
}

/* The same look as the settings dialog. */
.password {
  width: 340px;
  padding: 0;
  background: var(--toolbar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px #00000040;
  color: var(--text);
}

.password::backdrop {
  background: #00000040;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
}

h2,
p {
  margin: 0;
}

h2 {
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
}

.field,
.actions {
  display: flex;
  gap: 6px;
  align-items: center;
}

/* Both fields have the same width. The eye button is 22px wide. */
.field input {
  flex: 1;
  min-width: 0;
  margin-right: 28px;
}

.field input:not(:last-child) {
  margin-right: 0;
}

.actions {
  font-size: 11px;
}

.new-name {
  width: 140px;
}

.new-host {
  width: 90px;
}

.new-password {
  width: 140px;
}
</style>
