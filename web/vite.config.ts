import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: { include: ['src/**/*.test.ts'] },
  server: { proxy: { '/api': process.env.KYMO_API ?? 'http://127.0.0.1:8080' } },
})
