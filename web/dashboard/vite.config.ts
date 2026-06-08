import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
    plugins: [react()],
    server: {
        port: 3000,
        proxy: {
            '/api': {
                target: 'http://localhost:3002',
                changeOrigin: true,
            },
            '/ws': {
                target: 'ws://localhost:3002',
                ws: true,
            },
        },
    },
    build: {
        outDir: 'dist',
        // No source maps in production builds: they bloat the bundle and embed
        // third-party dependency source (including upstream authors' copyright
        // email addresses) into shipped artifacts.
        sourcemap: false,
    },
})
