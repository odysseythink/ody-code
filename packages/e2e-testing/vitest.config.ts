import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'e2e-testing',
    include: ['test/**/*.test.ts'],
  },
});
