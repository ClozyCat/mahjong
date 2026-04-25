import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
const spectatorEnabled = process.env.MAHJONG_ENABLE_SPECTATOR === 'true';
export default defineConfig({
    plugins: [react()],
    define: {
        __SPECTATOR_ENABLED__: JSON.stringify(spectatorEnabled),
    },
});
