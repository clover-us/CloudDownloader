// Harness-only dev config: same app, watcher disabled (Vite 8 `watch: null`).
// The editor's atomic-write temp dirs crash chokidar with EBUSY on Windows;
// the harness needs no HMR because every test run loads the page fresh.
// Run: npx vite --config harness/vite.config.ts
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: null,
  },
});
