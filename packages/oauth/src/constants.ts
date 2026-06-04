import type { OAuthFlowConfig } from './types';

export const ODY_CODE_FLOW_CONFIG: OAuthFlowConfig = {
  name: 'ody-code',
  oauthHost:
    process.env['ODY_CODE_OAUTH_HOST'] ??
    process.env['KIMI_OAUTH_HOST'] ??
    'https://auth.kimi.com',
  clientId: '17e5f671-d194-4dfb-9706-5516cb48c098',
};
