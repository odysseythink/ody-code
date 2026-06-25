import type { Agent } from '../../..';
import { PlanModeInjector } from '../../injection/plan-mode';
import { BaseSessionModeBehavior } from './base';
import type { SessionModeInjector } from './types';

export class PlanModeBehavior extends BaseSessionModeBehavior<'plan'> {
  readonly kind = 'plan' as const;
  readonly outputSubdirectory = 'plans';
  readonly modeModelKey = 'plan';
  readonly injectorClass = PlanModeInjector as unknown as new (agent: Agent) => SessionModeInjector;
  override readonly handoffTarget = 'normal' as const;
}
