import { defineConfig } from "nitro"

export default defineConfig({
  serverDir: './server',
  compatibilityDate: "2026-04-17",
  preset: "cloudflare_module",
  cloudflare: {
    deployConfig: true,
    nodeCompat: true
  },
  rollupConfig: {
    output: {
      inlineDynamicImports: true,
    }
  }
});
