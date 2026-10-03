import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

import { proxy } from './vite.proxy'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy,
  },
})
