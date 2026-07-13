import { describe, it, expect } from 'vitest';
import { ModeBehaviorRegistry, createDefaultModeBehaviorRegistry } from '../behaviors';
import { PlanModeBehavior } from '../behaviors/plan';
import { DesignModeBehavior } from '../behaviors/design';
import { ProductModeBehavior } from '../behaviors/product';
import { GameDesignModeBehavior } from '../behaviors/game-design';

describe('ModeBehaviorRegistry', () => {
  it('resolves registered behaviors by kind', () => {
    const registry = new ModeBehaviorRegistry();
    registry.register(new PlanModeBehavior());
    registry.register(new DesignModeBehavior());
    expect(registry.resolve('plan')).toBeInstanceOf(PlanModeBehavior);
    expect(registry.resolve('design')).toBeInstanceOf(DesignModeBehavior);
  });

  it('throws INTERNAL for unregistered kinds', () => {
    const registry = new ModeBehaviorRegistry();
    expect(() => registry.resolve('plan')).toThrow('Unknown session mode kind: plan');
  });

  it('lists registered kinds', () => {
    const registry = createDefaultModeBehaviorRegistry();
    expect(registry.kinds).toEqual(['plan', 'design', 'product', 'game-design']);
  });
});

describe('concrete behaviors', () => {
  it('has correct outputSubdirectory and modeModelKey for each kind', () => {
    expect(new PlanModeBehavior()).toMatchObject({ kind: 'plan', outputSubdirectory: 'plans', modeModelKey: 'plan' });
    expect(new DesignModeBehavior()).toMatchObject({ kind: 'design', outputSubdirectory: 'designs', modeModelKey: 'design' });
    expect(new ProductModeBehavior()).toMatchObject({ kind: 'product', outputSubdirectory: 'products', modeModelKey: 'product' });
    expect(new GameDesignModeBehavior()).toMatchObject({ kind: 'game-design', outputSubdirectory: 'game-design', modeModelKey: 'gameDesign' });
  });

  it('has correct handoff targets', () => {
    expect(new DesignModeBehavior().handoffTarget).toBe('plan');
    expect(new PlanModeBehavior().handoffTarget).toBe('normal');
    expect(new ProductModeBehavior().handoffTarget).toBeUndefined();
    expect(new GameDesignModeBehavior().handoffTarget).toBeUndefined();
  });

  it('tracks design sessions only for design', () => {
    expect(new DesignModeBehavior().supportsDesignSessions).toBe(true);
    expect(new PlanModeBehavior().supportsDesignSessions).toBeUndefined();
    expect(new ProductModeBehavior().supportsDesignSessions).toBeUndefined();
    expect(new GameDesignModeBehavior().supportsDesignSessions).toBeUndefined();
  });
});
