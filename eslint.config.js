import pluginVue from 'eslint-plugin-vue'
import { defineConfigWithVueTs, vueTsConfigs } from '@vue/eslint-config-typescript'
import skipFormatting from '@vue/eslint-config-prettier/skip-formatting'

const NO_ARROWS = {
  selector: 'ArrowFunctionExpression',
  message: 'Do not use arrow functions. Use a named function or a class method.',
}

export default defineConfigWithVueTs(
  { ignores: ['dist/**', 'src-tauri/**'] },
  pluginVue.configs['flat/essential'],
  vueTsConfigs.recommended,
  {
    rules: {
      // The same rule twice: once for scripts, once for Vue templates.
      'no-restricted-syntax': ['error', NO_ARROWS],
      'vue/no-restricted-syntax': ['error', NO_ARROWS],
      // A Session is a shared state object. Components may change its fields.
      'vue/no-mutating-props': ['error', { shallowOnly: true }],
      'vue/multi-word-component-names': 'off',
    },
  },
  skipFormatting,
)
