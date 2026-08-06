import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// This build ends up bundled into a servoshell build (see ../.github/workflows/test.yml's
// "assemble test bundle" step) and opened straight from a file:// URL, not served from an
// HTTP root — `base: "./"` keeps every asset reference relative so it still resolves there.
// Deliberately kept otherwise as close to a plain, default Vite app as possible (default
// code splitting, external <script type="module" src="...">, etc.): the point of this page
// is to check whether a normal Vite build works in this Servo build at all — see TODO.md's
// "Rischio critico" entry for what happens when it currently doesn't.
export default defineConfig({
  base: "./",
  plugins: [react()],
});
