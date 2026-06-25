import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vitest/config';

import { rawTextPlugin } from '../../build/raw-text-plugin.mjs';

export default defineConfig({
  plugins: [rawTextPlugin()],
  resolve: {
    alias: {
      '@odysseythink/agent-core': fileURLToPath(new URL('../agent-core/src/index.ts', import.meta.url)),
      '@odysseythink/kimi-code-oauth': fileURLToPath(
        new URL('../oauth/src/index.ts', import.meta.url),
      ),
    },
  },
  test: {
    name: 'kimi-sdk',
    include: ['test/**/*.test.ts', 'src/__tests__/**/*.test.ts'],
  },
});
