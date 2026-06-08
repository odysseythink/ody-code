import { vi, describe, expect, it, beforeEach } from 'vitest';
import puppeteer from 'puppeteer-core';
import { BrowserConnectionManager } from '../../src/browser/connection';
import { BrowserConnectionError } from '../../src/browser/types';

vi.mock('puppeteer-core', () => ({
  default: {
    connect: vi.fn(),
    launch: vi.fn(),
  },
}));

function createMockBrowser() {
  return {
    connected: true,
    newPage: vi.fn().mockResolvedValue({
      isClosed: () => false,
      close: vi.fn().mockResolvedValue(undefined),
    }),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

describe('BrowserConnectionManager', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('connects to existing Chrome on port 9222', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    const handle = await manager.resolveOrLaunchBrowser();

    expect(handle.kind).toBe('connected');
    expect(puppeteer.connect).toHaveBeenCalledWith(
      expect.objectContaining({ browserURL: 'http://127.0.0.1:9222' }),
    );
    expect(puppeteer.launch).not.toHaveBeenCalled();
  });

  it('tries custom chromePort when provided', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager({ chromePort: 9333 });
    await manager.resolveOrLaunchBrowser();

    expect(puppeteer.connect).toHaveBeenCalledWith(
      expect.objectContaining({ browserURL: 'http://127.0.0.1:9333' }),
    );
    expect(puppeteer.connect).toHaveBeenCalledTimes(1);
  });

  it('falls back to launch when connect fails and autoLaunch=true', async () => {
    vi.mocked(puppeteer.connect).mockRejectedValue(new Error('Connection failed'));
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.launch).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.launch>);

    const manager = new BrowserConnectionManager();
    const handle = await manager.resolveOrLaunchBrowser();

    expect(handle.kind).toBe('launched');
    expect(puppeteer.launch).toHaveBeenCalledWith(
      expect.objectContaining({ headless: true, defaultViewport: null }),
    );
  });

  it('passes executablePath to launch when configured', async () => {
    vi.mocked(puppeteer.connect).mockRejectedValue(new Error('Connection failed'));
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.launch).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.launch>);

    const manager = new BrowserConnectionManager({
      executablePath: '/usr/bin/google-chrome',
    });
    await manager.resolveOrLaunchBrowser();

    expect(puppeteer.launch).toHaveBeenCalledWith(
      expect.objectContaining({ executablePath: '/usr/bin/google-chrome' }),
    );
  });

  it('throws BrowserConnectionError when connect fails and autoLaunch=false', async () => {
    vi.mocked(puppeteer.connect).mockRejectedValue(new Error('Connection failed'));

    const manager = new BrowserConnectionManager({ autoLaunch: false });
    await expect(manager.resolveOrLaunchBrowser()).rejects.toBeInstanceOf(BrowserConnectionError);
    await expect(manager.resolveOrLaunchBrowser()).rejects.toThrow('No browser available');
  });

  it('returns same handle on subsequent calls (singleton)', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    const handle1 = await manager.resolveOrLaunchBrowser();
    const handle2 = await manager.resolveOrLaunchBrowser();

    expect(handle1).toBe(handle2);
    expect(puppeteer.connect).toHaveBeenCalledTimes(1);
  });

  it('reconnects when active browser disconnects', async () => {
    const mockBrowser1 = { ...createMockBrowser(), connected: false };
    const mockBrowser2 = createMockBrowser();

    vi.mocked(puppeteer.connect)
      .mockResolvedValueOnce(mockBrowser1 as unknown as ReturnType<typeof puppeteer.connect>)
      .mockResolvedValueOnce(mockBrowser2 as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    const handle1 = await manager.resolveOrLaunchBrowser();
    expect(handle1.browser).toBe(mockBrowser1);

    const handle2 = await manager.resolveOrLaunchBrowser();
    expect(handle2.browser).toBe(mockBrowser2);
    expect(puppeteer.connect).toHaveBeenCalledTimes(2);
  });

  it('closeAll closes browser and clears active handle', async () => {
    const mockBrowser = createMockBrowser();
    vi.mocked(puppeteer.connect).mockResolvedValue(mockBrowser as unknown as ReturnType<typeof puppeteer.connect>);

    const manager = new BrowserConnectionManager();
    await manager.resolveOrLaunchBrowser();
    await manager.closeAll();

    expect(mockBrowser.close).toHaveBeenCalled();
    expect(manager.getActiveHandle()).toBeUndefined();
  });
});
