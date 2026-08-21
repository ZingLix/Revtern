import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, new URL(".", import.meta.url).pathname, "");
  const apiTarget = env.VITE_API_BASE_URL || "http://127.0.0.1:3001";

  return {
    plugins: [react(), tailwindcss()],
    server: {
      port: 3000,
      proxy: {
        "/api": {
          target: apiTarget,
          changeOrigin: true,
        },
        "/webhooks": {
          target: apiTarget,
          changeOrigin: true,
        },
      },
    },
  };
});
