import type { Agent } from '../../..';
import { GameDesignInjector } from '../../injection/game-design';
import { BaseSessionModeBehavior } from './base';
import type { SessionModeInjector } from './types';

export class GameDesignModeBehavior extends BaseSessionModeBehavior<'game-design'> {
  readonly kind = 'game-design' as const;
  readonly outputSubdirectory = 'game-design';
  readonly modeModelKey = 'gameDesign';
  readonly injectorClass = GameDesignInjector as unknown as new (agent: Agent) => SessionModeInjector;
}
