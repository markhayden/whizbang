// The build of the playground (ADR 0042). Fixed file names, no source maps and no hash in
// a name, so the host can embed a fixed list of files (crates/whizbang-serve/src/play/
// assets.rs) and the same sources give the same bytes: `scripts/ui.sh check` builds again
// and compares with the committed ui/dist/.
import { defineConfig } from "vitest/config";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact({ devToolsEnabled: false, prefreshEnabled: false })],
  base: "/",
  publicDir: "public",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    modulePreload: { polyfill: false },
    cssCodeSplit: false,
    assetsInlineLimit: 0,
    reportCompressedSize: false,
    rollupOptions: {
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "app-[name].js",
        assetFileNames: (asset) => (asset.names.some((name) => name.endsWith(".css")) ? "app.css" : "assets/[name][extname]"),
      },
    },
  },
  test: {
    environment: "happy-dom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    css: false,
    testTimeout: 5000,
    pool: "threads",
    maxWorkers: 2,
    reporters: ["default"],
  },
});
