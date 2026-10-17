<script setup lang="ts">
import { computed, onMounted, reactive, ref, useTemplateRef, watch } from 'vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { EyeIcon, EyeOffIcon, KeyRoundIcon, Trash2Icon, UsersIcon } from 'lucide-vue-next'
import {
  ACCESS_LABELS,
  ACCESS_ORDER,
  DEFAULT_USER_HOST,
  GRANTABLE_ACCESS,
} from '../constants/users'
import type { Session } from '../services/Session'
import { UserList } from '../services/UserList'
import type { DbUser, GrantableAccess, GridColumn, Row } from '../types'
import BottomBar from './BottomBar.vue'
import DataGrid from './DataGrid.vue'

const props = defineProps<{ session: Session }>()
const hasHosts = computed(usesHosts)
const list = reactive(new UserList())
const columns = computed(gridColumns)
const shown = computed(showUsers)
const rows = computed(gridRows)
const selected = ref<number[]>([])
const target = computed(selectedUser)
const accessTitle = computed(describeAccessSelect)
const removeTitle = computed(describeRemove)
/** The user to select again after the rows move. Set only while an access change runs. */
let keepKey = ''
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
// A new sort or load moves the rows. The old selection would point to other rows.
watch(shown, selectAfterMove)

function reload(): void {
  props.session.loadUsers()
}

function sortColumn(name: string): void {
  list.sortByName(name)
}

/** Clears the selection. After an access change the same user stays selected. */
function selectAfterMove(): void {
  const index = keepKey ? shown.value.findIndex(isKept) : -1
  selected.value = index === -1 ? [] : [index]
}

function isKept(user: DbUser): boolean {
  return UserList.key(user) === keepKey
}

function gridColumns(): GridColumn[] {
  return UserList.columns(hasHosts.value)
}

function showUsers(): DbUser[] {
  return list.visible(props.session.users)
}

function gridRows(): Row[] {
  const rows: Row[] = []
  for (const user of shown.value) {
    rows.push(UserList.row(user, hasHosts.value))
  }
  return rows
}

/** The last selected user. */
function selectedUser(): DbUser | null {
  const index = selected.value.at(-1)
  return (index === undefined ? undefined : shown.value[index]) ?? null
}

function describeAccessSelect(): string {
  const user = target.value
  if (!user) {
    return 'Select a user first'
  }
  return lockReason(user) || `Access of ${user.name}`
}

function describeRemove(): string {
  const user = target.value
  if (!user) {
    return 'Select a user first'
  }
  return user.current ? 'You cannot remove your own account' : 'Remove the selected user'
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

function changeAccess(event: Event): void {
  const user = target.value
  const select = event.target as HTMLSelectElement
  if (!user) {
    return
  }
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
  if (!(await confirm(question, { title: 'Change access', kind: 'warning' }))) {
    return
  }
  // The new list comes in before setUserAccess returns. So the key is read in time.
  keepKey = UserList.key(user)
  try {
    await props.session.setUserAccess(user, access)
  } finally {
    keepKey = ''
  }
}

function removeSelected(): void {
  if (target.value && !target.value.current) {
    props.session.run(remove(target.value))
  }
}

function changeSelectedPassword(): void {
  if (target.value) {
    openPasswordDialog(target.value)
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
    <span class="spacer" />
    <select
      :value="target?.access ?? ''"
      :title="accessTitle"
      :disabled="!target || lockReason(target) !== ''"
      @change="changeAccess"
    >
      <option v-if="!target" value="" disabled>Access</option>
      <option
        v-else-if="!GRANTABLE_ACCESS.includes(target.access as GrantableAccess)"
        :value="target.access"
        disabled
      >
        {{ ACCESS_LABELS[target.access] }}
      </option>
      <option v-for="level in GRANTABLE_ACCESS" :key="level" :value="level">
        {{ ACCESS_LABELS[level] }}
      </option>
    </select>
    <button
      class="icon"
      :title="target ? 'Change the password of the selected user' : 'Select a user first'"
      :disabled="!target"
      @click="changeSelectedPassword"
    >
      <KeyRoundIcon :size="13" />
    </button>
    <button
      class="icon danger"
      :title="removeTitle"
      :disabled="!target || target.current"
      @click="removeSelected"
    >
      <Trash2Icon :size="13" />
    </button>
  </div>
  <DataGrid
    v-model:selected="selected"
    :columns="columns"
    :rows="rows"
    :order-by="list.columnName"
    :descending="list.descending"
    @sort="sortColumn"
  />

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

/* The global danger colour wins over the disabled grey. Keep the normal disabled look. */
button.danger:disabled {
  color: var(--text-3);
}

.bar select {
  height: 22px;
  border-radius: 5px;
  font-size: 11px;
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
