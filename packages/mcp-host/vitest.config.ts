import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'mcp-host',
    include: ['test/**/*.test.ts'],
  },
});
