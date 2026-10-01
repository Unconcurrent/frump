import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  esbuild: { supported: { 'template-literal': false } },
  server: { proxy: { '/api': 'http://127.0.0.1:3333' } },
  build: {
    outDir: '../src/web/dist',
    emptyOutDir: true,
    rollupOptions: {
      output: { entryFileNames: 'assets/app.js', assetFileNames: 'assets/app.[ext]' },
    },
  },
});
