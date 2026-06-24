import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { toInputJsonSchema } from '../src/input-schema';

describe('toInputJsonSchema', () => {
  it('renders a basic object schema as input JSON Schema', () => {
    const schema = z.object({ name: z.string(), count: z.number().optional() });
    const result = toInputJsonSchema(schema);
    expect(result).toHaveProperty('type', 'object');
    expect(result).toHaveProperty('properties');
    expect(result).toHaveProperty('additionalProperties', false);
  });

  it('keeps defaulted fields out of required', () => {
    const schema = z.object({ name: z.string(), active: z.boolean().default(true) });
    const result = toInputJsonSchema(schema);
    const required = (result as Record<string, unknown>)['required'] as string[] | undefined;
    expect(required).toContain('name');
    expect(required).not.toContain('active');
  });

  it('sets additionalProperties: false on object nodes', () => {
    const schema = z.object({
      nested: z.object({ inner: z.string() }),
    });
    const result = toInputJsonSchema(schema);
    const properties = (result as Record<string, unknown>)['properties'] as Record<string, unknown>;
    const nested = properties['nested'] as Record<string, unknown>;
    expect(nested).toHaveProperty('additionalProperties', false);
  });
});
