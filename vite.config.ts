import { defineConfig } from "vite";

// Tauri expects a stable local dev server address.
export default defineConfig({
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
});
