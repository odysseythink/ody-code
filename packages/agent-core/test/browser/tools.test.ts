import { vi, describe, expect, it, beforeEach } from 'vitest';
import {
  BrowserBrowseTool,
  BrowserExtractTool,
  BrowserActTool,
  BrowserNavigateTool,
  BrowserSnapshotTool,
} from '../../src/tools/builtin/browser';
import type { BrowserConnectionManager } from '../../src/browser/connection';
import type { BrowserHandle } from '../../src/browser/types';

function createMockPage(overrides: Partial<{
  goto: typeof vi.fn;
  title: typeof vi.fn;
  url: typeof vi.fn;
  evaluate: typeof vi.fn;
  screenshot: typeof vi.fn;
  click: typeof vi.fn;
  type: typeof vi.fn;
  waitForSelector: typeof vi.fn;
  $: typeof vi.fn;
} > = {}) {
  return {
    goto: vi.fn().mockResolvedValue(undefined),
    title: vi.fn().mockResolvedValue('Test Page'),
    url: vi.fn().mockReturnValue('https://example.com/'),
    evaluate: vi.fn().mockResolvedValue({}),
    screenshot: vi.fn().mockResolvedValue('base64screenshot'),
    click: vi.fn().mockResolvedValue(undefined),
    type: vi.fn().mockResolvedValue(undefined),
    waitForSelector: vi.fn().mockResolvedValue(undefined),
    $: vi.fn().mockResolvedValue({
      evaluate: vi.fn().mockResolvedValue('element text'),
    }),
    ...overrides,
  };
}

function createMockHandle(page: ReturnType<typeof createMockPage>): BrowserHandle {
  return {
    id: 'test',
    kind: 'connected',
    browser: { connected: true, close: vi.fn() } as unknown as BrowserHandle['browser'],
    defaultPage: page as unknown as BrowserHandle['defaultPage'],
    acquirePage: vi.fn().mockResolvedValue(page),
    releasePage: vi.fn(),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

describe('Browser tools', () => {
  let mockConnection: BrowserConnectionManager;
  let mockPage: ReturnType<typeof createMockPage>;
  let mockHandle: BrowserHandle;

  beforeEach(() => {
    vi.resetAllMocks();
    mockPage = createMockPage();
    mockHandle = createMockHandle(mockPage);
    mockConnection = {
      resolveOrLaunchBrowser: vi.fn().mockResolvedValue(mockHandle),
      getActiveHandle: vi.fn(),
      closeAll: vi.fn(),
    } as unknown as BrowserConnectionManager;
  });

  describe('BrowserBrowseTool', () => {
    it('navigates and returns page info', async () => {
      mockPage.evaluate.mockResolvedValue('page content');
      const tool = new BrowserBrowseTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://example.com' });

      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(mockPage.goto).toHaveBeenCalledWith('https://example.com', expect.any(Object));
      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Test Page');
      expect(result.output).toContain('example.com');
    });

    it('sets approvalRule with host', () => {
      const tool = new BrowserBrowseTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://kimi.com/code' });
      expect(execution.approvalRule).toBe('Browser*(kimi.com)');
      expect(execution.matchesRule?.('kimi.com')).toBe(true);
      expect(execution.matchesRule?.('evil.kimi.com')).toBe(false);
    });

    it('returns error for invalid URL', async () => {
      const tool = new BrowserBrowseTool(mockConnection);
      // URL validation happens in schema parse, but resolveExecution would throw
      expect(() => tool.resolveExecution({ url: 'not-a-url' } as unknown as { url: string })).toThrow();
    });
  });

  describe('BrowserNavigateTool', () => {
    it('navigates to URL', async () => {
      const tool = new BrowserNavigateTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://example.com' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(mockPage.goto).toHaveBeenCalledWith('https://example.com', expect.any(Object));
      expect(result.isError).toBeFalsy();
    });

    it('sets Browser*(host) approvalRule', () => {
      const tool = new BrowserNavigateTool(mockConnection);
      const execution = tool.resolveExecution({ url: 'https://kimi.com' });
      expect(execution.approvalRule).toBe('Browser*(kimi.com)');
    });
  });

  describe('BrowserSnapshotTool', () => {
    it('returns page text content', async () => {
      mockPage.evaluate.mockResolvedValue('Hello world');
      const tool = new BrowserSnapshotTool(mockConnection);
      const execution = tool.resolveExecution({});
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Hello world');
    });

    it('returns error for missing selector', async () => {
      mockPage.$.mockResolvedValue(null);
      const tool = new BrowserSnapshotTool(mockConnection);
      const execution = tool.resolveExecution({ selector: '#missing' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBe(true);
      expect(result.output).toContain('Element not found');
    });
  });

  describe('BrowserExtractTool', () => {
    it('extracts data using schema', async () => {
      mockPage.evaluate.mockResolvedValue({ title: 'Hello', body: 'World' });
      const tool = new BrowserExtractTool(mockConnection);
      const execution = tool.resolveExecution({
        schema: { title: 'h1', body: 'p' },
      });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Hello');
      expect(result.output).toContain('World');
    });

    it('uses tool name approvalRule when no URL', () => {
      const tool = new BrowserExtractTool(mockConnection);
      const execution = tool.resolveExecution({ schema: { title: 'h1' } });
      expect(execution.approvalRule).toBe('BrowserExtract');
      expect(execution.matchesRule).toBeUndefined();
    });

    it('uses Browser*(host) when URL provided', () => {
      const tool = new BrowserExtractTool(mockConnection);
      const execution = tool.resolveExecution({
        url: 'https://kimi.com',
        schema: { title: 'h1' },
      });
      expect(execution.approvalRule).toBe('Browser*(kimi.com)');
      expect(execution.matchesRule?.('kimi.com')).toBe(true);
    });
  });

  describe('BrowserActTool', () => {
    it('clicks element', async () => {
      const tool = new BrowserActTool(mockConnection);
      const execution = tool.resolveExecution({ action: 'click', selector: '#btn' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(mockPage.click).toHaveBeenCalledWith('#btn');
      expect(result.isError).toBeFalsy();
    });

    it('returns error for click without selector', async () => {
      const tool = new BrowserActTool(mockConnection);
      const execution = tool.resolveExecution({ action: 'click' });
      const result = await execution.execute({ turnId: '1', toolCallId: 'c1', metadata: {}, signal: new AbortController().signal, onUpdate: vi.fn() });

      expect(result.isError).toBe(true);
      expect(result.output).toContain('requires a selector');
    });

    it('uses tool name approvalRule', () => {
      const tool = new BrowserActTool(mockConnection);
      const execution = tool.resolveExecution({ action: 'scroll_down' });
      expect(execution.approvalRule).toBe('BrowserAct');
    });
  });
});
