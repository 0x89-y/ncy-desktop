import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Cargo's workspace target now lives at the repository root. On Windows,
      // watching a DLL while rustc/link.exe replaces it raises EBUSY and kills
      // Vite's beforeDevCommand. Rust/Tauri already watches both Rust crates.
      ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
    },
  },
});
