import { describe, expect, it } from 'vitest';
import { isValidPermissionPattern, parsePattern } from '../src/permission-pattern';

describe('permission pattern parser', () => {
  it('parses tool-name-only patterns', () => {
    expect(parsePattern('Write')).toEqual({ toolName: 'Write' });
  });

  it('parses arg patterns', () => {
    expect(parsePattern('Read(/etc/**)')).toEqual({ toolName: 'Read', argPattern: '/etc/**' });
  });

  it('validates well-formed patterns', () => {
    expect(isValidPermissionPattern('Bash(!rm *)')).toBe(true);
  });

  it('rejects malformed patterns', () => {
    expect(isValidPermissionPattern('')).toBe(false);
    expect(isValidPermissionPattern('Read(/etc')).toBe(false);
  });
});
