import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";

export default defineConfig({
  plugins: [react(), tailwindcss(), viteSingleFile()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  base: "./",
  build: {
    outDir: "dist",
    target: "esnext",
    minify: "esbuild",
    modulePreload: false,
    cssCodeSplit: false,
    assetsInlineLimit: 100000000,
  },
});
