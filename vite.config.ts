import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 8788,
    strictPort: true,
    host: true,
    proxy: {
      '/ag-image': 'http://127.0.0.1:8789',
      '/cursor-image': 'http://127.0.0.1:8789',
      '/media': 'http://127.0.0.1:8789',
    },
  },
})
