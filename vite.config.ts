import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],

  // `glass-ui` is a file: dependency with react/react-dom as peerDependencies
  // (a nested node_modules copy exists under its checkout); force every import
  // of them onto this app's single copy so hooks never resolve against a
  // second React instance — the config screen (the first GlassSwitch render)
  // otherwise crashes with "Cannot read properties of null (reading
  // 'useState')", React unmounts the tree and the transparent window goes
  // fully invisible.
  resolve: {
    dedupe: ["react", "react-dom", "react/jsx-runtime"],
  },

  clearScreen: false,
  server: {
    port: 5180,
    strictPort: true,
  },
  build: {
    target: "es2021",
  },
});
