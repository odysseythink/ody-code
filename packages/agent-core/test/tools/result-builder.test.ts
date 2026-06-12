import { describe, expect, it } from 'vitest';

import { ToolResultBuilder } from '../../src/tools/support/result-builder';

describe('ToolResultBuilder', () => {
  it('returns concatenated output and a confirmation message', () => {
    const builder = new ToolResultBuilder();

    builder.write('Hello');
    builder.write(' world');

    const result = builder.ok('Operation completed');
    expect(result.output).toBe('Hello world');
    expect(result.message).toBe('Operation completed');
    expect(result.isError).toBe(false);
    expect(builder.nChars).toBe(11);
  });

  it('truncates long lines when maxLineLength is set', () => {
    const builder = new ToolResultBuilder({ maxLineLength: 20 });

    builder.write('This is a very long line that should be truncated\n');

    const result = builder.ok();
    expect(result.output).toContain('This is a very long …');
  });

  it('respects maxLineLength for multiple lines', () => {
    const builder = new ToolResultBuilder({ maxLineLength: 20 });

    builder.write('Line 1\n');
    builder.write('This is a very long line that exceeds limit\n');
    builder.write('More text');

    const result = builder.ok();
    expect(result.output).toContain('Line 1\n');
    expect(result.output).toContain('This is a very long …');
    expect(result.output).toContain('More text');
  });

  it('tracks nChars as the buffer grows', () => {
    const builder = new ToolResultBuilder({ maxLineLength: 30 });

    expect(builder.nChars).toBe(0);

    builder.write('Short\n');
    expect(builder.nChars).toBe(6);

    builder.write('1\n2\n');
    expect(builder.nChars).toBe(10);

    builder.write('More text that exceeds');
    expect(builder.nChars).toBe(32);
  });

  it('treats an empty write as a no-op', () => {
    const builder = new ToolResultBuilder();

    builder.write('');
    expect(builder.nChars).toBe(0);
  });

  it('returns the accumulated output with the supplied message and brief', () => {
    const builder = new ToolResultBuilder();

    builder.write('Some output');
    const result = builder.error('Something went wrong', { brief: 'Error occurred' });

    expect(result.output).toContain('Some output');
    expect(result.message).toBe('Error occurred');
    expect(result.isError).toBe(true);
  });

  it('preserves an explicitly empty brief', () => {
    const builder = new ToolResultBuilder();

    const result = builder.ok('Done', { brief: '' });

    expect(result.message).toBe('');
  });

  it('preserves the message and brief together on error', () => {
    const builder = new ToolResultBuilder();

    builder.write('Very long output that exceeds limit');
    const result = builder.error('Command failed', { brief: 'Failed' });

    expect(result.output).toContain('Very long output that exceeds limit');
    expect(result.message).toBe('Failed');
    expect(result.isError).toBe(true);
  });

  it('keeps normal success messages out of non-empty output', () => {
    const builder = new ToolResultBuilder();

    builder.write('ok\n');
    const result = builder.ok('Command executed successfully.');

    expect(result.output).toBe('ok\n');
    expect(result.message).toBe('Command executed successfully.');
  });
});
