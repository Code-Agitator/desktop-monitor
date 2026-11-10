import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "chrome105",
    rollupOptions: {
      input: {
        main: "index.html",
        settings: "settings.html",
        hud: "hud.html",
        taskbar: "taskbar.html",
        dev: "dev.html",
      },
    },
  },
});
