import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'agent-core-shared',
    include: ['test/**/*.test.ts'],
  },
});
