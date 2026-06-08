import puppeteer, { type Browser, type Page, type LaunchOptions } from 'puppeteer-core';
import type { Logger } from '#/logging/types';
import {
  BrowserConnectionError,
  type BrowserConnectionOptions,
  type BrowserHandle,
} from './types';

export class BrowserConnectionManager {
  private activeHandle?: BrowserHandle;
  private readonly options: Required<Pick<BrowserConnectionOptions, 'autoLaunch' | 'headless'>> &
    BrowserConnectionOptions;
  private readonly log?: Logger;

  constructor(options: BrowserConnectionOptions = {}) {
    this.options = {
      autoLaunch: true,
      headless: true,
      ...options,
    };
    this.log = options.log;
  }

  async resolveOrLaunchBrowser(): Promise<BrowserHandle> {
    if (this.activeHandle) {
      try {
        if (this.activeHandle.browser.connected) {
          return this.activeHandle;
        }
      } catch {
        // Browser disconnected, continue to reconnect
      }
    }

    this.activeHandle = undefined;

    // PRIMARY: Try to connect to existing Chrome
    const connectedHandle = await this.tryConnect();
    if (connectedHandle) {
      this.activeHandle = connectedHandle;
      return connectedHandle;
    }

    // FALLBACK: Launch new browser
    if (this.options.autoLaunch) {
      const launchedHandle = await this.tryLaunch();
      if (launchedHandle) {
        this.activeHandle = launchedHandle;
        return launchedHandle;
      }
    }

    throw new BrowserConnectionError(
      'No browser available. Please start Chrome with --remote-debugging-port, ' +
        'or set browser.autoLaunch=true in config.',
    );
  }

  getActiveHandle(): BrowserHandle | undefined {
    return this.activeHandle;
  }

  async closeAll(): Promise<void> {
    const handle = this.activeHandle;
    this.activeHandle = undefined;
    if (handle) {
      await handle.close().catch(() => {});
    }
  }

  private async tryConnect(): Promise<BrowserHandle | undefined> {
    const ports = this.options.chromePort
      ? [this.options.chromePort]
      : [9222, 9223, 9224];

    for (const port of ports) {
      try {
        const browser = await puppeteer.connect({
          browserURL: `http://127.0.0.1:${port}`,
          defaultViewport: null,
        });
        return await this.createHandle(browser, 'connected');
      } catch (error) {
        this.log?.debug(`browser connect failed on port ${port}`, error);
        // Try next port
      }
    }
    return undefined;
  }

  private async tryLaunch(): Promise<BrowserHandle | undefined> {
    try {
      const launchOptions: LaunchOptions = {
        headless: this.options.headless,
        defaultViewport: null,
      };
      if (this.options.executablePath) {
        launchOptions.executablePath = this.options.executablePath;
      }
      if (this.options.userDataDir) {
        launchOptions.userDataDir = this.options.userDataDir;
      }
      const browser = await puppeteer.launch(launchOptions);
      return await this.createHandle(browser, 'launched');
    } catch (error) {
      this.log?.debug('browser launch failed', error);
      return undefined;
    }
  }

  private async createHandle(
    browser: Browser,
    kind: 'connected' | 'launched',
  ): Promise<BrowserHandle> {
    const id = `${kind}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const defaultPage = await browser.newPage();
    const pagePool: Page[] = [];
    const acquiredPages = new Set<Page>([defaultPage]);

    return {
      id,
      kind,
      browser,
      defaultPage,
      acquirePage: async () => {
        const released = pagePool.pop();
        if (released && !released.isClosed()) {
          acquiredPages.add(released);
          return released;
        }
        const page = await browser.newPage();
        acquiredPages.add(page);
        return page;
      },
      releasePage: (page: Page) => {
        acquiredPages.delete(page);
        if (page !== defaultPage && !page.isClosed()) {
          pagePool.push(page);
        }
      },
      close: async () => {
        for (const page of [...pagePool, ...acquiredPages]) {
          if (!page.isClosed()) {
            await page.close().catch(() => {});
          }
        }
        pagePool.length = 0;
        acquiredPages.clear();
        if (kind === 'connected') {
          await browser.disconnect().catch(() => {});
        } else if (browser.connected) {
          await browser.close().catch(() => {});
        }
      },
    };
  }
}
