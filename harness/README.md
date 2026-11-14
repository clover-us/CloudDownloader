# UI harness

Browser-only smoke tests for the account/drive UI. Runs the real Vue app against
a mock Tauri IPC layer (`mock-tauri.ts`) — no Tauri runtime, no real app data.

## Run

```powershell
npm ci                      # app deps
npm i --no-save playwright  # test-only dep (not in package.json)
npx vite --config harness/vite.config.ts   # dev server on :1420 (watcher disabled)
node harness/test.mjs       # full check run, screenshots in harness/shots/
```

Uses system Edge via Playwright (`channel: "msedge"`), falls back to bundled
Chromium. `vite.config.ts` here disables the file watcher (`watch: null`)
because editor atomic-write temp dirs crash chokidar with EBUSY on Windows;
tests reload the page fresh, so HMR is not needed.

## Coverage

Connected-only account rows, add-account dialog (search / sections / tiles),
login flow + 4s poll promotion, drive switcher pills + auto-switch on logout,
logout confirm, night theme, empty states, initial loading state, 920px layout,
download-task folder grouping (expand/collapse, aggregate progress, search),
batch select / pause / resume / remove, upload-list batch select / pause /
clear-completed / remove, console-error guard.
