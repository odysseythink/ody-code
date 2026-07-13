import { ErrorCodes, OdyError } from '@odysseythink/agent-core-shared';
import type { SessionModeKind } from '../types';
import { DesignModeBehavior } from './design';
import { GameDesignModeBehavior } from './game-design';
import { ProductModeBehavior } from './product';
import { PlanModeBehavior } from './plan';
import type { SessionModeBehavior } from './types';

export class ModeBehaviorRegistry {
  private readonly behaviors = new Map<SessionModeKind, SessionModeBehavior<SessionModeKind>>();

  register<T extends SessionModeKind>(behavior: SessionModeBehavior<T>): void {
    this.behaviors.set(behavior.kind, behavior);
  }

  resolve(kind: SessionModeKind): SessionModeBehavior<SessionModeKind> {
    const behavior = this.behaviors.get(kind);
    if (behavior === undefined) {
      throw new OdyError(ErrorCodes.INTERNAL, `Unknown session mode kind: ${kind}`);
    }
    return behavior;
  }

  get kinds(): readonly SessionModeKind[] {
    return Array.from(this.behaviors.keys());
  }
}

export function createDefaultModeBehaviorRegistry(): ModeBehaviorRegistry {
  const registry = new ModeBehaviorRegistry();
  registry.register(new PlanModeBehavior());
  registry.register(new DesignModeBehavior());
  registry.register(new ProductModeBehavior());
  registry.register(new GameDesignModeBehavior());
  return registry;
}
