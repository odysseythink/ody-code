import { describe, expect, it } from 'vitest';
import { parseOdyMarker, renderDebtLedger } from '#/tools/builtin/code-quality/harvest-ody-markers';

describe('parseOdyMarker', () => {
  it('parses a valid // marker with upgrade trigger', () => {
    const result = parseOdyMarker(
      'src/lock.ts:12:// ody: 全局锁, 吞吐 > 100 rps 时改为按账户锁',
    );
    expect(result).toEqual({
      file: 'src/lock.ts',
      line: 12,
      ceiling: '全局锁',
      upgrade: '吞吐 > 100 rps 时改为按账户锁',
      rot: false,
    });
  });

  it('parses a valid # marker with upgrade trigger', () => {
    const result = parseOdyMarker(
      'scripts/parse.py:8:# ody: 用 JSON.parse, 需要 schema 校验时改为 zod',
    );
    expect(result).toEqual({
      file: 'scripts/parse.py',
      line: 8,
      ceiling: '用 JSON.parse',
      upgrade: '需要 schema 校验时改为 zod',
      rot: false,
    });
  });

  it('marks rot when upgrade trigger is missing (no comma)', () => {
    const result = parseOdyMarker('src/cache.ts:5:// ody: 全局锁');
    expect(result).toEqual({
      file: 'src/cache.ts',
      line: 5,
      ceiling: '全局锁',
      upgrade: '',
      rot: true,
    });
  });

  it('marks rot when upgrade trigger is empty after comma', () => {
    const result = parseOdyMarker('src/cache.ts:5:// ody: 全局锁,   ');
    expect(result).toEqual({
      file: 'src/cache.ts',
      line: 5,
      ceiling: '全局锁',
      upgrade: '',
      rot: true,
    });
  });

  it('marks rot for Chinese comma (not a valid separator)', () => {
    const result = parseOdyMarker(
      'src/lock.ts:12:// ody: 全局锁，吞吐 > 100 rps 时改为按账户锁',
    );
    expect(result!.rot).toBe(true);
  });

  it('returns null for lines without ody: prefix', () => {
    expect(parseOdyMarker('src/lock.ts:12:// TODO: fix lock')).toBeNull();
    expect(parseOdyMarker('src/lock.ts:12:// body: foo')).toBeNull();
  });

  it('returns null for block comments (unsupported)', () => {
    expect(
      parseOdyMarker('src/lock.ts:12:/* ody: 全局锁, upgrade */'),
    ).toBeNull();
  });

  it('handles optional space before ody:', () => {
    const result = parseOdyMarker(
      'src/x.ts:7://ody: simple, more complex when needed',
    );
    expect(result).toEqual({
      file: 'src/x.ts',
      line: 7,
      ceiling: 'simple',
      upgrade: 'more complex when needed',
      rot: false,
    });
  });
});

describe('renderDebtLedger', () => {
  it('returns clean message for empty markers', () => {
    const result = renderDebtLedger([], false);
    expect(result).toBe('未找到 `ody:` 债务标记。台账干净。');
  });

  it('renders grouped markers with rot tag', () => {
    const markers = [
      {
        file: 'src/lock.ts',
        line: 12,
        ceiling: '全局锁',
        upgrade: '吞吐 > 100 rps 时改为按账户锁',
        rot: false,
      },
      {
        file: 'src/lock.ts',
        line: 45,
        ceiling: '临时文件',
        upgrade: '',
        rot: true,
      },
    ];
    const result = renderDebtLedger(markers, false);

    expect(result).toContain('### src/lock.ts');
    expect(result).toContain('src/lock.ts:12');
    expect(result).toContain('全局锁');
    expect(result).toContain('吞吐 > 100 rps 时改为按账户锁');
    expect(result).toContain('src/lock.ts:45');
    expect(result).toContain('⚠️ rot');
    expect(result).toContain('（未指定）');
    expect(result).toContain('**汇总**：2 个标记，1 个 rot 风险。');
  });

  it('sorts files alphabetically and lines within each file', () => {
    const markers = [
      {
        file: 'zzz/last.ts', line: 1, ceiling: 'c', upgrade: 'u', rot: false,
      },
      {
        file: 'aaa/first.ts', line: 3, ceiling: 'c', upgrade: 'u', rot: false,
      },
      {
        file: 'aaa/first.ts', line: 1, ceiling: 'c', upgrade: 'u', rot: false,
      },
    ];
    const result = renderDebtLedger(markers, false);

    const aaaIdx = result.indexOf('### aaa/first.ts');
    const zzzIdx = result.indexOf('### zzz/last.ts');
    expect(aaaIdx).toBeLessThan(zzzIdx);

    const line1Idx = result.indexOf('aaa/first.ts:1');
    const line3Idx = result.indexOf('aaa/first.ts:3');
    expect(line1Idx).toBeLessThan(line3Idx);
  });

  it('appends truncated hint when truncated is true', () => {
    const markers = [
      {
        file: 'src/x.ts', line: 1, ceiling: 'c', upgrade: 'u', rot: false,
      },
    ];
    const result = renderDebtLedger(markers, true);
    expect(result).toContain('结果已截断至前 200 条');
  });

  it('shows zero rot when all markers have upgrade triggers', () => {
    const markers = [
      {
        file: 'a.ts', line: 1, ceiling: 'c1', upgrade: 'u1', rot: false,
      },
      {
        file: 'b.ts', line: 1, ceiling: 'c2', upgrade: 'u2', rot: false,
      },
    ];
    const result = renderDebtLedger(markers, false);
    expect(result).toContain('**汇总**：2 个标记，0 个 rot 风险。');
  });
});
