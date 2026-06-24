import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'code-review',
    include: ['test/**/*.test.ts'],
  },
});
