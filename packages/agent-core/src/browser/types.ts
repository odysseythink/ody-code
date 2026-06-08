import { z } from 'zod';
import type { Browser, Page } from 'puppeteer-core';

// ─── Connection Options ───

export interface BrowserConnectionOptions {
  /** Explicit remote debugging port for existing Chrome. */
  chromePort?: number;
  /** Whether to auto-launch a new browser if connection fails. Default: true. */
  autoLaunch?: boolean;
  /** Headless mode for launched instances. Default: true. */
  headless?: boolean;
  /** Explicit Chrome/Chromium executable path for launch. */
  executablePath?: string;
  /** User data dir for launched instances. */
  userDataDir?: string;
  /** Optional logger for connection diagnostics. */
  log?: import('#/logging/types').Logger;
}

// ─── Browser Handle ───

export interface BrowserHandle {
  readonly id: string;
  readonly kind: 'connected' | 'launched';
  readonly browser: Browser;
  readonly defaultPage: Page;
  acquirePage(): Promise<Page>;
  releasePage(page: Page): void;
  close(): Promise<void>;
}

// ─── Tool Input Schemas ───

export const BrowserBrowseInputSchema = z.object({
  url: z.string().url(),
  goal: z.string().optional(),
  waitFor: z.union([z.string(), z.number().int().min(0)]).optional(),
  extract: z.record(z.string(), z.string()).optional(),
});

export type BrowserBrowseInput = z.infer<typeof BrowserBrowseInputSchema>;

export const BrowserExtractInputSchema = z.object({
  url: z.string().url().optional(),
  schema: z.record(z.string(), z.string()),
});

export type BrowserExtractInput = z.infer<typeof BrowserExtractInputSchema>;

export const BrowserActInputSchema = z.object({
  action: z.enum(['click', 'type', 'scroll_down', 'scroll_up', 'screenshot', 'wait']),
  selector: z.string().optional(),
  value: z.string().optional(),
});

export type BrowserActInput = z.infer<typeof BrowserActInputSchema>;

export const BrowserNavigateInputSchema = z.object({
  url: z.string().url(),
});

export type BrowserNavigateInput = z.infer<typeof BrowserNavigateInputSchema>;

export const BrowserSnapshotInputSchema = z.object({
  selector: z.string().optional(),
});

export type BrowserSnapshotInput = z.infer<typeof BrowserSnapshotInputSchema>;

export const BrowserClickInputSchema = z.object({
  selector: z.string(),
});

export type BrowserClickInput = z.infer<typeof BrowserClickInputSchema>;

export const BrowserFillInputSchema = z.object({
  selector: z.string(),
  value: z.string(),
});

export type BrowserFillInput = z.infer<typeof BrowserFillInputSchema>;

export const BrowserEvaluateInputSchema = z.object({
  script: z.string(),
});

export type BrowserEvaluateInput = z.infer<typeof BrowserEvaluateInputSchema>;

export const BrowserScreenshotInputSchema = z.object({
  fullPage: z.boolean().optional(),
});

export type BrowserScreenshotInput = z.infer<typeof BrowserScreenshotInputSchema>;

// ─── Tool Output ───

export interface BrowserToolOutput {
  readonly success: boolean;
  readonly url: string;
  readonly title: string;
  readonly content?: string;
  readonly data?: unknown;
  readonly screenshot?: string;
  readonly error?: string;
}

// ─── Error ───

export class BrowserConnectionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'BrowserConnectionError';
  }
}
