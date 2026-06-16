import { ErrorCodes } from '@odysseythink/ody-code-sdk';

export const PRODUCT_NAME = 'Ody Code';
export const CLI_COMMAND_NAME = 'ody';

// Used in telemetry app names and HTTP User-Agent headers.
export const CLI_USER_AGENT_PRODUCT = 'kimi-code-cli';
export const CLI_UI_MODE = 'shell';

// Give telemetry a short flush window without making CLI exit feel stuck.
export const CLI_SHUTDOWN_TIMEOUT_MS = 3000;

// Published npm package name; this can differ from the executable command.
export const NPM_PACKAGE_NAME = 'ody-code';

// App-owned data paths. SDK/core runtime config is intentionally not routed here.
export const ODY_CODE_HOME_ENV = 'ODY_CODE_HOME';
export const ODY_CODE_DATA_DIR_NAME = '.ody-code';
export const ODY_CODE_LOG_DIR_NAME = 'logs';
export const ODY_CODE_UPDATE_DIR_NAME = 'updates';
export const ODY_CODE_UPDATE_STATE_FILE_NAME = 'latest.json';
export const ODY_CODE_UPDATE_INSTALL_STATE_FILE_NAME = 'install.json';
export const ODY_CODE_UPDATE_INSTALL_LOCK_FILE_NAME = 'install.lock';
export const ODY_CODE_INPUT_HISTORY_DIR_NAME = 'user-history';

// Managed Kimi auth provider key shared with OAuth/SDK config.
export const DEFAULT_OAUTH_PROVIDER_NAME = 'managed:ody-code';

// SDK/core error code that tells the TUI to show a login-required startup
// notice. Derived from sdk's ErrorCodes so a future rename in core
// auto-propagates instead of silently breaking the startup recovery path.
export const OAUTH_LOGIN_REQUIRED_CODE = ErrorCodes.AUTH_LOGIN_REQUIRED;

export const FEEDBACK_ISSUE_URL = 'https://github.com/odysseythink/ody-code/issues';

// Sent in the feedback `version` field so the backend can distinguish this
// TypeScript client from clients that send a bare version.
export const FEEDBACK_VERSION_PREFIX = 'ody-code-';

// Telemetry event name; keep stable for dashboard queries.
export const FEEDBACK_TELEMETRY_EVENT = 'feedback_submitted';

// CDN source of truth: all version checks and native install scripts pull from here.
export const ODY_CODE_CDN_BASE = 'https://code.ody.com/ody-code';
export const ODY_CODE_CDN_LATEST_URL = `${ODY_CODE_CDN_BASE}/latest`;
export const ODY_CODE_PLUGIN_MARKETPLACE_URL = `${ODY_CODE_CDN_BASE}/plugins/marketplace.json`;
export const ODY_CODE_PLUGIN_MARKETPLACE_URL_ENV = 'ODY_CODE_PLUGIN_MARKETPLACE_URL';
export const ODY_CODE_INSTALL_SH_URL = `${ODY_CODE_CDN_BASE}/install.sh`;
export const ODY_CODE_INSTALL_PS1_URL = `${ODY_CODE_CDN_BASE}/install.ps1`;

// Native install commands, split by platform. Use these for prompt copy and spawn calls only; do not assemble the strings elsewhere.
export const NATIVE_INSTALL_COMMAND_UNIX = `curl -fsSL ${ODY_CODE_INSTALL_SH_URL} | bash`;
export const NATIVE_INSTALL_COMMAND_WIN = `irm ${ODY_CODE_INSTALL_PS1_URL} | iex`;
