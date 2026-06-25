import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'ody-crypto',
    include: ['test/**/*.test.ts'],
  },
});
