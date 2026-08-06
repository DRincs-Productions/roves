import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// This build ends up bundled into a servoshell build (see ../.github/workflows/test.yml's
// "assemble test bundle" step) and opened straight from disk, not served from an HTTP
// root — `base: "./"` keeps every asset reference relative so it still resolves there.
export default defineConfig({
  base: "./",
  plugins: [react()],
});
