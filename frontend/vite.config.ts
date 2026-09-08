import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri serves the built frontend over a custom protocol (tauri://localhost).
// Vite's default absolute asset paths ("/assets/..") + the `crossorigin`
// attribute get blocked as cross-origin on that protocol, producing a blank
// white screen. Using a relative base and stripping `crossorigin` fixes it.
function stripCrossorigin() {
  return {
    name: "strip-crossorigin",
    transformIndexHtml(html: string) {
      return html.replace(/\s+crossorigin/g, "");
    },
  };
}

export default defineConfig({
  plugins: [react(), stripCrossorigin()],
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
  },
});
