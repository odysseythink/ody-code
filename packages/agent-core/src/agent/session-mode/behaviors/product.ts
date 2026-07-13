import type { Agent } from '../../..';
import { ProductInjector } from '../../injection/product';
import { BaseSessionModeBehavior } from './base';
import type { SessionModeInjector } from './types';

export class ProductModeBehavior extends BaseSessionModeBehavior<'product'> {
  readonly kind = 'product' as const;
  readonly outputSubdirectory = 'products';
  readonly modeModelKey = 'product';
  readonly injectorClass = ProductInjector as unknown as new (agent: Agent) => SessionModeInjector;
}
