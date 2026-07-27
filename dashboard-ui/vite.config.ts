import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { readdir } from 'node:fs/promises'
import { fileURLToPath, URL } from 'node:url'

const EXPECTED_DIST_FILES = [
  'index.html',
  'pm-dashboard.css',
  'pm-dashboard.js',
]

export default defineConfig({
  base: '/assets/dashboard/',
  plugins: [
    react(),
    tailwindcss(),
    {
      name: 'verify-embedded-dashboard-assets',
      apply: 'build',
      async closeBundle() {
        const distPath = fileURLToPath(new URL('./dist', import.meta.url))
        const files = (await readdir(distPath)).sort()
        if (files.join('\n') !== EXPECTED_DIST_FILES.join('\n')) {
          throw new Error(
            `Embedded dashboard assets changed: expected ${EXPECTED_DIST_FILES.join(', ')}, received ${files.join(', ')}`,
          )
        }
      },
    },
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    cssCodeSplit: false,
    rolldownOptions: {
      output: {
        entryFileNames: 'pm-dashboard.js',
        assetFileNames: 'pm-dashboard[extname]',
        codeSplitting: false,
      },
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: './src/setup-tests.ts',
    css: true,
  },
})
