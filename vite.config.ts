import {defineConfig} from 'vitest/config';
export default defineConfig({clearScreen:false,build:{outDir:'ui-dist',sourcemap:false},test:{environment:'jsdom',include:['src/**/*.test.ts']}});
