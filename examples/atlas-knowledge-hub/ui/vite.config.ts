import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const target = 'http://localhost:8080'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': target,
      '/admin': target,
      '/evidence': target,
      '/demo': target,
      '/ws': { target, ws: true },
      '/health': target,
      '/ready': target,
    },
  },
})
