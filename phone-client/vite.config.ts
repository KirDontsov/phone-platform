import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
export default defineConfig({plugins:[vue(),tailwindcss()],server:{port:5173,proxy:{'/api':{target:process.env.API_PROXY || 'http://127.0.0.1:8080',changeOrigin:false}}},test:{environment:'jsdom',include:['tests/**/*.test.ts'],exclude:['tests/e2e/**']}})
