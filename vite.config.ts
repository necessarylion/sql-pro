import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
// @ts-expect-error type error without @types/node package
import process from 'node:process'

const host = process.env.TAURI_DEV_HOST

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  // Do not hide Rust errors.
  clearScreen: false,
  // Tauri expects a fixed port.
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
})
