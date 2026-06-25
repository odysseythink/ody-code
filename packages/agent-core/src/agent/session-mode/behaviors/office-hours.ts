import type { Agent } from '../../..';
import { OfficeHoursInjector } from '../../injection/office-hours';
import { BaseSessionModeBehavior } from './base';
import type { SessionModeInjector } from './types';

export class OfficeHoursModeBehavior extends BaseSessionModeBehavior<'office-hours'> {
  readonly kind = 'office-hours' as const;
  readonly outputSubdirectory = 'products';
  readonly modeModelKey = 'officeHours';
  readonly injectorClass = OfficeHoursInjector as unknown as new (agent: Agent) => SessionModeInjector;
}
