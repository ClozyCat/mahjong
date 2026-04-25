import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
export default defineConfig({
    plugins: [react()],
    define: {
        __SPECTATOR_ENABLED__: JSON.stringify(process.env.MAHJONG_ENABLE_SPECTATOR === 'true'),
    },
    test: {
        environment: 'jsdom',
        globals: true,
        setupFiles: './src/test/setup.ts',
        css: true,
    },
});
