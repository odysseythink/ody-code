/**
 * ExitDesignModeTool tests against the current Agent-backed tool surface.
 */

import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import {
  ExitDesignModeInputSchema,
  ExitDesignModeTool,
  findMissingDesignSections,
} from '../../src/tools/builtin/planning/exit-design-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

describe('findMissingDesignSections', () => {
  it('returns empty array for a complete design', () => {
    const design = `## Scope In/Out
Content here is sufficient. This is the scope definition section that describes what is in and out of scope.

## Architecture
More content here describing the architecture. We need to have sufficient content to meet the 300 character minimum requirement.

## Data Models
Additional content to reach 300 chars minimum. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum length requirement.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;
    expect(findMissingDesignSections(design)).toEqual([]);
  });

  it('detects content shorter than 300 chars', () => {
    const result = findMissingDesignSections('short');
    expect(result.some((m) => m.includes('sufficient content'))).toBe(true);
  });

  it('detects fewer than 3 ## sections', () => {
    const design = `## Scope
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor.

## Architecture
More content to reach minimum.`;
    const result = findMissingDesignSections(design);
    expect(result.some((m) => m.includes('at least 3 design sections'))).toBe(true);
  });

  it('detects missing Scope section', () => {
    const design = `## Architecture
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Models
More content here for minimum length.

## Errors
And final section to exceed 300 chars requirement and ensure test passes.`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Scope or Scope In/Out section');
  });

  it('detects missing Architecture section', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Models
More content here for minimum length.

## Errors
And final section to exceed 300 chars requirement and ensure test passes.`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Architecture or Design section');
  });

  it('accepts "Design" as alternative to "Architecture"', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod.

## Design
Alternative architecture heading. Sed do eiusmod tempor incididunt ut labore et dolore.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;
    expect(findMissingDesignSections(design)).toEqual([]);
  });

  it('accepts "Approach" as alternative to "Architecture"', () => {
    const design = `## Scope In/Out
Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Approach
Alternative heading works. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;
    expect(findMissingDesignSections(design)).toEqual([]);
  });

  it('accepts Chinese "架构" as alternative to Architecture', () => {
    const design = `## 范围定义
内容内容内容。这是一个关于设计的内容说明。Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## 架构
设计架构部分。Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation.

## 数据模型
更多内容以满足最小长度要求。Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;
    expect(findMissingDesignSections(design)).toEqual([]);
  });

  // C3: missing Data Models section
  it('detects missing Data Models section', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Algorithms
Algorithm details with enough content to exceed three hundred characters minimum.

## Error Handling
Error handling content.`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Data Models section');
  });

  // C4: missing Algorithms section
  it('detects missing Algorithms section', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Error Handling
Error handling strategies and fallback paths.`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Algorithms section');
  });

  // C5: missing Error Handling section
  it('detects missing Error Handling section', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode and control flow details with sufficient content.`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Error Handling section');
  });

  // C6: missing Self-Review section
  it('detects missing Self-Review section', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail.`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('Self-Review section');
  });

  // C7: missing User Approval marker
  it('detects missing User Approval', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths.

## Self-Review
Security: checked X. Test: checked Y.`;
    const result = findMissingDesignSections(design);
    expect(result).toContain('User Approval');
  });

  // All 7 pass
  it('returns empty for a design with all 7 criteria met', () => {
    const design = `## Scope In/Out
Content. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor.

## Architecture
Architecture content here. Ut labore et dolore magna aliqua. Ut enim ad minim veniam.

## Data Models
Data model definitions with enough text for the minimum length requirement.

## Algorithms
Algorithm pseudocode with sufficient content to exceed the minimum.

## Error Handling
Error handling strategies and fallback paths with enough detail text.

## Self-Review
Security: checked X. Test: checked Y. Ops: verified Z.

## User Final Approval
Approved by user [C:USER].`;
    expect(findMissingDesignSections(design)).toEqual([]);
  });

  it('returns multiple missing items', () => {
    const design = 'Too short';
    const result = findMissingDesignSections(design);
    expect(result.length).toBeGreaterThan(1);
  });
});

const COMPLETE_DESIGN = `
## Scope In/Out

### Scope In
- Feature A [C:USER]

### Scope Out
- Feature B [C:DEFERRED]

## Architecture

The system uses X to accomplish Y. Call site: \`src/foo.ts:42\`.

## Data Models

\`\`\`ts
interface Foo { id: string; }
\`\`\`

## Algorithms

1. Parse input
2. Validate against schema
3. Transform and return

## Error Handling

| Error | Strategy |
|-------|----------|
| ENOENT | return null |

## Self-Review

Security: verified no secrets in paths. Test: all assertions traceable.

## User Final Approval

Approved by user [C:USER].
`.trim();

function makeAgent(
  input: {
    readonly active?: boolean | undefined;
    readonly design?: string | null | undefined;
    readonly path?: string | undefined;
    readonly sessionModeFilePath?: string | null | undefined;
    readonly emit?: ((event: unknown) => void) | undefined;
  } = {},
): { agent: Agent; requestApproval: ReturnType<typeof vi.fn>; emit: ReturnType<typeof vi.fn>; handoffTo: ReturnType<typeof vi.fn> } {
  let active = input.active ?? true;
  const requestApproval = vi.fn(async () => ({ decision: 'approved' }));
  const emit = vi.fn((event: unknown) => {
    input.emit?.(event);
    if ((event as { type?: string }).type === 'session_mode.exit') active = false;
  });
  const handoffTo = vi.fn(async () => {
    emit({ type: 'session_mode.exit' });
  });
  const agent = {
    sessionMode: {
      get isActive() {
        return active;
      },
      get sessionModeFilePath() {
        return input.sessionModeFilePath ?? null;
      },
      data: vi.fn(async () => {
        if (input.design === null) return null;
        return {
          content: input.design ?? 'Step 1: brainstorm\nStep 2: evaluate',
          path: input.path ?? '/tmp/kimi-design.md',
        };
      }),
      finalizeFileName: vi.fn().mockResolvedValue(null),
      handoffTo,
      exit: () => {
        emit({ type: 'session_mode.exit' });
      },
    },
    rpc: { requestApproval },
    telemetry: { track: vi.fn() },
    emit,
  } as unknown as Agent;
  return { agent, requestApproval, emit, handoffTo };
}

describe('ExitDesignModeTool', () => {
  it('has name, description, and parameters from the current schema', () => {
    const { agent } = makeAgent();
    const tool = new ExitDesignModeTool(agent);

    expect(tool.name).toBe('ExitDesignMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(ExitDesignModeInputSchema.safeParse({}).success).toBe(true);
    expect(ExitDesignModeInputSchema.safeParse({ plan: '' }).success).toBe(false);
    expect(tool.parameters).toMatchObject({
      type: 'object',
      properties: {
        options: { type: 'array' },
      },
    });
  });

  it('refuses to exit when design mode is inactive', async () => {
    const { agent, emit } = makeAgent({ active: false });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('design mode');
    expect(emit).not.toHaveBeenCalled();
  });

  it('exits with the current design without consulting permission approval', async () => {
    const { agent, requestApproval, emit, handoffTo } = makeAgent({
      design: COMPLETE_DESIGN,
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result.isError).toBe(false);
    expect(requestApproval).not.toHaveBeenCalled();
    expect(emit).toHaveBeenCalledWith({ type: 'session_mode.exit' });
    expect(result.output).toContain('Design saved to: /tmp/kimi-design.md');
    expect(result.output).toContain('Design mode deactivated');
    expect(result.output).not.toContain('## Scope In/Out');
    expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: undefined });
  });

  it('passes the declared selected label to handoffTo', async () => {
    const { agent, handoffTo } = makeAgent({
      design: COMPLETE_DESIGN,
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_label',
      args: { options: [{ label: 'Approach A', description: 'Do A' }] },
      signal,
      metadata: { selectedLabel: 'Approach A' },
    });

    expect(result.isError).toBe(false);
    expect(result.output).toContain('Selected approach: Approach A');
    expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: 'Approach A' });
  });

  it('does not use inline design fallback when no design file exists', async () => {
    const { agent, emit } = makeAgent({ design: null });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_inline',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(emit).not.toHaveBeenCalled();
    expect(result.output).toContain('No design file found');
  });

  it('rejects empty design content (completeness gate)', async () => {
    const { agent } = makeAgent({
      design: '',
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_empty',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('incomplete');
    expect(result.output).toContain('sufficient content');
  });

  it('surfaces errors from design exit as a tool error', async () => {
    const { agent, handoffTo } = makeAgent({ design: COMPLETE_DESIGN });
    handoffTo.mockRejectedValue(new Error('journal write failed'));

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_fail',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('journal write failed');
  });

  it('rejects when content is too short (< 300 chars)', async () => {
    const { agent } = makeAgent({
      design: '## Scope\n\nFoo\n\n## Design\n\nBar',
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_short',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('incomplete');
    expect(result.output).toContain('sufficient content');
  });

  it('rejects when fewer than 3 ## sections present', async () => {
    const { agent } = makeAgent({
      design: `## Scope In/Out
Very long content to exceed 300 chars. Lorem ipsum dolor sit amet, consectetur adipiscing elit.
Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Architecture
More content to reach minimum length requirement. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.`,
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_sections',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('at least 3 design sections');
    expect(result.output).toContain('found 2');
  });

  it('rejects when Scope section is absent', async () => {
    const { agent } = makeAgent({
      design: `## Architecture
Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.

## Data Models
Interface definitions go here. Let's add more content to exceed the 300 character minimum requirement and ensure we have enough text.

## Error Handling
Error scenarios and handling strategies.`,
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_noscope',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('Scope or Scope In/Out section');
  });

  it('rejects when Architecture section is absent', async () => {
    const { agent } = makeAgent({
      design: `## Scope In/Out
Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Data Models
Interface definitions and structures. Let's add more content to exceed minimum requirements. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.

## Error Handling
Error scenarios and handling strategies. We need to ensure sufficient content for the test.`,
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_noarch',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(result.output).toContain('Architecture or Design section');
  });

  it('passes with a complete design that meets all criteria', async () => {
    const { agent, handoffTo } = makeAgent({
      design: COMPLETE_DESIGN,
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_complete',
      args: {},
      signal,
    });

    expect(result.isError).toBe(false);
    expect(result.output).toContain('Design saved to');
    expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: undefined });
  });

  it('does NOT apply the completeness gate when design mode is inactive', async () => {
    const { agent } = makeAgent({
      active: false,
      design: '',
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_inactive',
      args: {},
      signal,
    });

    // Inactive check fires first (in execution, not gate), returns "design mode" error
    expect(result.isError).toBe(true);
    expect(result.output).toContain('design mode');
    expect(result.output).not.toContain('incomplete');
  });

  describe('option description optionality', () => {
    it('exposes options[].description as optional with a default of empty string', () => {
      const { agent } = makeAgent();
      const tool = new ExitDesignModeTool(agent);

      const optionItems = (
        (tool.parameters['properties'] as Record<string, unknown>)['options'] as {
          items?: {
            required?: readonly string[];
            properties?: Record<string, { default?: unknown }>;
          };
        }
      ).items;

      expect(optionItems?.required).toEqual(['label']);
      expect(optionItems?.required).not.toContain('description');
      expect(optionItems?.properties?.['description']?.default).toBe('');
    });

    it('accepts an option that omits description', () => {
      const result = ExitDesignModeInputSchema.safeParse({
        options: [{ label: 'Approach A' }],
      });

      expect(result.success).toBe(true);
    });
  }); // closes 'option description optionality' describe
}); // closes 'ExitDesignModeTool' describe
